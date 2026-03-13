use super::*;

pub(crate) fn cleanup_native_blob_storage(
    blob_storage_dir: &Path,
    retention_days: u64,
) -> Result<usize> {
    if !blob_storage_dir.exists() {
        return Ok(0);
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(
            retention_days.saturating_mul(24 * 60 * 60),
        ))
        .unwrap_or(UNIX_EPOCH);
    let mut removed = 0usize;
    prune_blob_dir(blob_storage_dir, blob_storage_dir, cutoff, &mut removed)?;
    Ok(removed)
}

fn prune_blob_dir(root: &Path, dir: &Path, cutoff: SystemTime, removed: &mut usize) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|error| {
        RuntimeHardeningError::new(
            "native_runtime_blob_cleanup_failed",
            format!(
                "Failed to read native blob storage '{}': {error}",
                dir.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_blob_cleanup_failed",
                format!(
                    "Failed to enumerate native blob storage '{}': {error}",
                    dir.display()
                ),
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_blob_cleanup_failed",
                format!(
                    "Failed to inspect native blob entry '{}': {error}",
                    path.display()
                ),
            )
        })?;
        if file_type.is_dir() {
            prune_blob_dir(root, &path, cutoff, removed)?;
            if path != root && directory_is_empty(&path)? {
                fs::remove_dir(&path).map_err(|error| {
                    RuntimeHardeningError::new(
                        "native_runtime_blob_cleanup_failed",
                        format!(
                            "Failed to remove empty native blob dir '{}': {error}",
                            path.display()
                        ),
                    )
                })?;
            }
            continue;
        }
        if !file_type.is_file() || path.extension().and_then(|value| value.to_str()) != Some("blob")
        {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_blob_cleanup_failed",
                    format!(
                        "Failed to read native blob metadata '{}': {error}",
                        path.display()
                    ),
                )
            })?;
        if modified <= cutoff {
            fs::remove_file(&path).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_blob_cleanup_failed",
                    format!(
                        "Failed to remove expired native blob '{}': {error}",
                        path.display()
                    ),
                )
            })?;
            *removed = removed.saturating_add(1);
        }
    }
    Ok(())
}

fn directory_is_empty(path: &Path) -> Result<bool> {
    let mut entries = fs::read_dir(path).map_err(|error| {
        RuntimeHardeningError::new(
            "native_runtime_blob_cleanup_failed",
            format!(
                "Failed to inspect native blob dir '{}': {error}",
                path.display()
            ),
        )
    })?;
    Ok(entries
        .next()
        .transpose()
        .map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_blob_cleanup_failed",
                format!(
                    "Failed to inspect native blob dir '{}': {error}",
                    path.display()
                ),
            )
        })?
        .is_none())
}

impl NativeRuntimeSupport {
    pub fn bootstrap(env: &HashMap<String, String>) -> Result<Self> {
        let config = RuntimeHardeningConfig::from_env(env);
        let owner_token = format!("native-runtime-{}", Uuid::new_v4());
        let mut readiness = NativeReadinessGate::new();
        let state_token = readiness.register("runtime_state_db");
        let secrets_token = readiness.register("secrets_manager");
        let log_token = readiness.register("runtime_log_ingestor");

        let state_store = match NativeRuntimeStateStore::open(&config) {
            Ok(store) => {
                readiness.mark_ready(&state_token, "state_db_initialized");
                store
            }
            Err(error) => {
                readiness.mark_failed(&state_token, error.message.clone());
                return Err(error);
            }
        };

        let secrets = match NativeSecretsManager::open(&config, env) {
            Ok(manager) => {
                readiness.mark_ready(&secrets_token, "secrets_manager_initialized");
                manager
            }
            Err(error) => {
                readiness.mark_failed(&secrets_token, error.message.clone());
                return Err(error);
            }
        };

        let acquired = state_store.acquire_lease(
            LOG_INGESTOR_LEASE_NAME,
            &owner_token,
            config.lease_ttl_ms,
        )?;
        if !acquired {
            readiness.mark_failed(
                &log_token,
                "runtime log ingestor lease is already held by another owner",
            );
            return Err(RuntimeHardeningError::new(
                "native_runtime_log_ingestor_lease_denied",
                "Runtime log ingestor lease is already held by another owner",
            ));
        }
        let log_ingestor =
            match RuntimeLogIngestor::start(state_store.clone(), owner_token.clone(), &config) {
                Ok(ingestor) => {
                    readiness.mark_ready(&log_token, "runtime_log_ingestor_started");
                    ingestor
                }
                Err(error) => {
                    readiness.mark_failed(&log_token, error.message.clone());
                    return Err(error);
                }
            };

        if !readiness.all_ready() {
            return Err(RuntimeHardeningError::new(
                "native_runtime_not_ready",
                "Native runtime components failed readiness gate",
            ));
        }

        Ok(Self {
            log_ingestor,
            secrets,
            readiness,
            telemetry: NativeRuntimeStateTelemetry {
                db_path: config.state_db_path.to_string_lossy().to_string(),
                busy_timeout_ms: config.busy_timeout_ms,
                log_batch_size: config.log_batch_size,
                log_retention_days: config.log_retention_days,
                lease_owner_token: owner_token,
            },
        })
    }

    #[must_use]
    pub fn readiness_transitions(&self) -> Vec<NativeReadinessTransition> {
        self.readiness.transitions()
    }

    #[must_use]
    pub fn telemetry(&self) -> NativeRuntimeStateTelemetry {
        self.telemetry.clone()
    }

    pub fn ensure_secret_from_or_to_env(
        &self,
        env: &mut HashMap<String, String>,
        key: &str,
    ) -> Result<()> {
        if let Some(value) = env
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            self.secrets.set_secret(key, value)?;
            return Ok(());
        }
        if let Some(stored) = self.secrets.get_secret(key)? {
            env.insert(key.to_string(), stored);
        }
        Ok(())
    }

    pub fn ingest_log(
        &self,
        component: impl Into<String>,
        level: impl Into<String>,
        message: impl Into<String>,
        context_json: Option<String>,
    ) -> Result<()> {
        self.log_ingestor.log(RuntimeLogRecord {
            ts_ms: now_ms(),
            component: component.into(),
            level: level.into(),
            message: message.into(),
            context_json,
        })
    }

    pub fn flush_logs(&self) -> Result<()> {
        self.log_ingestor.flush()
    }

    pub fn shutdown(self) -> Result<()> {
        self.log_ingestor.shutdown()
    }
}
