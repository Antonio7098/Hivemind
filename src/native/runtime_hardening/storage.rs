use super::*;

#[derive(Debug, Clone)]
pub(super) struct NativeRuntimeStateStore {
    pub(super) db_path: PathBuf,
    busy_timeout_ms: u64,
}

impl NativeRuntimeStateStore {
    pub(super) fn open(config: &RuntimeHardeningConfig) -> Result<Self> {
        if let Some(parent) = config.state_db_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_dir_create_failed",
                    format!(
                        "Failed to create native runtime state dir '{}': {error}",
                        parent.display()
                    ),
                )
            })?;
        }

        let store = Self {
            db_path: config.state_db_path.clone(),
            busy_timeout_ms: config.busy_timeout_ms,
        };
        store.apply_pragmas_and_migrations()?;
        Ok(store)
    }

    fn apply_pragmas_and_migrations(&self) -> Result<()> {
        let setup_sql = format!(
            "PRAGMA journal_mode=WAL;\
             PRAGMA busy_timeout={};\
             CREATE TABLE IF NOT EXISTS schema_migrations (\
               version INTEGER PRIMARY KEY,\
               applied_at_ms INTEGER NOT NULL\
             );",
            self.busy_timeout_ms
        );
        self.exec(&setup_sql)?;

        const MIGRATIONS: &[(i64, &str)] = &[
            (
                1,
                "CREATE TABLE IF NOT EXISTS runtime_logs (\
                   id INTEGER PRIMARY KEY AUTOINCREMENT,\
                   ts_ms INTEGER NOT NULL,\
                   component TEXT NOT NULL,\
                   level TEXT NOT NULL,\
                   message TEXT NOT NULL,\
                   context_json TEXT,\
                   created_at_ms INTEGER NOT NULL\
                 );\
                 CREATE INDEX IF NOT EXISTS idx_runtime_logs_ts ON runtime_logs(ts_ms);",
            ),
            (
                2,
                "CREATE TABLE IF NOT EXISTS job_leases (\
                   job_name TEXT PRIMARY KEY,\
                   owner_token TEXT NOT NULL,\
                   acquired_at_ms INTEGER NOT NULL,\
                   heartbeat_at_ms INTEGER NOT NULL,\
                   lease_expires_at_ms INTEGER NOT NULL\
                 );",
            ),
        ];

        for (version, sql) in MIGRATIONS {
            if self.migration_applied(*version)? {
                continue;
            }
            let applied_at = now_ms();
            let migration_sql = format!(
                "PRAGMA busy_timeout={};\
                 BEGIN IMMEDIATE;\
                 {}\
                 INSERT OR IGNORE INTO schema_migrations(version, applied_at_ms) VALUES({}, {});\
                 COMMIT;",
                self.busy_timeout_ms, sql, version, applied_at
            );
            self.exec(&migration_sql)?;
        }

        Ok(())
    }

    fn migration_applied(&self, version: i64) -> Result<bool> {
        let query = format!(
            "PRAGMA busy_timeout={};\
             SELECT COUNT(1) FROM schema_migrations WHERE version={version};",
            self.busy_timeout_ms
        );
        Ok(self.scalar_i64(&query)? > 0)
    }

    pub(super) fn acquire_lease(
        &self,
        job_name: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<bool> {
        let now = now_ms();
        let ttl = i64::try_from(ttl_ms).unwrap_or(i64::MAX);
        let expires = now.saturating_add(ttl);
        let sql = format!(
            "PRAGMA busy_timeout={};\
             BEGIN IMMEDIATE;\
             INSERT INTO job_leases(job_name, owner_token, acquired_at_ms, heartbeat_at_ms, lease_expires_at_ms)\
             VALUES('{job_name}', '{owner_token}', {now}, {now}, {expires})\
             ON CONFLICT(job_name) DO UPDATE SET \
               owner_token=excluded.owner_token,\
               heartbeat_at_ms=excluded.heartbeat_at_ms,\
               lease_expires_at_ms=excluded.lease_expires_at_ms \
             WHERE job_leases.owner_token=excluded.owner_token \
                OR job_leases.lease_expires_at_ms < {now};\
             COMMIT;",
            self.busy_timeout_ms,
            job_name = sql_escape(job_name),
            owner_token = sql_escape(owner_token),
        );
        self.exec(&sql)?;

        let owner_query = format!(
            "PRAGMA busy_timeout={};\
             SELECT owner_token FROM job_leases WHERE job_name='{}' LIMIT 1;",
            self.busy_timeout_ms,
            sql_escape(job_name)
        );
        let current_owner = self.scalar_string(&owner_query)?;
        Ok(current_owner.as_deref() == Some(owner_token))
    }

    fn heartbeat_lease(&self, job_name: &str, owner_token: &str, ttl_ms: u64) -> Result<()> {
        let now = now_ms();
        let ttl = i64::try_from(ttl_ms).unwrap_or(i64::MAX);
        let expires = now.saturating_add(ttl);
        let sql = format!(
            "PRAGMA busy_timeout={};\
             UPDATE job_leases \
             SET heartbeat_at_ms={now}, lease_expires_at_ms={expires} \
             WHERE job_name='{}' AND owner_token='{}';",
            self.busy_timeout_ms,
            sql_escape(job_name),
            sql_escape(owner_token),
        );
        self.exec(&sql)
    }

    fn release_lease(&self, job_name: &str, owner_token: &str) -> Result<()> {
        let sql = format!(
            "PRAGMA busy_timeout={};\
             DELETE FROM job_leases WHERE job_name='{}' AND owner_token='{}';",
            self.busy_timeout_ms,
            sql_escape(job_name),
            sql_escape(owner_token),
        );
        self.exec(&sql)
    }

    fn ingest_logs(&self, logs: &[RuntimeLogRecord]) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }
        let mut values = String::new();
        for (index, log) in logs.iter().enumerate() {
            if index > 0 {
                values.push(',');
            }
            let context = log.context_json.as_deref().unwrap_or_default();
            let created_at = now_ms();
            values.push_str(&format!(
                "({},'{}','{}','{}','{}',{})",
                log.ts_ms,
                sql_escape(&log.component),
                sql_escape(&log.level),
                sql_escape(&log.message),
                sql_escape(context),
                created_at
            ));
        }
        let sql = format!(
            "PRAGMA busy_timeout={};\
             BEGIN IMMEDIATE;\
             INSERT INTO runtime_logs(ts_ms, component, level, message, context_json, created_at_ms)\
             VALUES {};\
             COMMIT;",
            self.busy_timeout_ms, values
        );
        self.exec(&sql)
    }

    fn cleanup_logs(&self, retention_days: u64) -> Result<u64> {
        let retention_ms = i64::try_from(retention_days)
            .unwrap_or(i64::MAX)
            .saturating_mul(24)
            .saturating_mul(60)
            .saturating_mul(60)
            .saturating_mul(1000);
        let cutoff = now_ms().saturating_sub(retention_ms);
        let sql = format!(
            "PRAGMA busy_timeout={};\
             DELETE FROM runtime_logs WHERE ts_ms < {cutoff};\
             SELECT changes();",
            self.busy_timeout_ms
        );
        let deleted = self.scalar_i64(&sql)?;
        Ok(u64::try_from(deleted).unwrap_or(0))
    }

    fn exec(&self, sql: &str) -> Result<()> {
        let output = Command::new("sqlite3")
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_exec_failed",
                    format!(
                        "Failed to invoke sqlite3 for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "sqlite3 command failed with empty stderr".to_string()
        } else {
            stderr
        };
        Err(RuntimeHardeningError::new(
            "native_runtime_state_sql_exec_failed",
            format!("sqlite3 failed for '{}': {message}", self.db_path.display()),
        ))
    }

    fn scalar_i64(&self, sql: &str) -> Result<i64> {
        let output = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_query_failed",
                    format!(
                        "Failed to invoke sqlite3 query for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RuntimeHardeningError::new(
                "native_runtime_state_sql_query_failed",
                format!(
                    "sqlite3 query failed for '{}': {}",
                    self.db_path.display(),
                    if stderr.is_empty() {
                        "empty stderr".to_string()
                    } else {
                        stderr
                    }
                ),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let value = raw
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string();
        value.parse::<i64>().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_state_sql_parse_failed",
                format!("Failed to parse sqlite integer output '{raw}': {error}"),
            )
        })
    }

    fn scalar_string(&self, sql: &str) -> Result<Option<String>> {
        let output = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_query_failed",
                    format!(
                        "Failed to invoke sqlite3 query for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RuntimeHardeningError::new(
                "native_runtime_state_sql_query_failed",
                format!(
                    "sqlite3 query failed for '{}': {}",
                    self.db_path.display(),
                    if stderr.is_empty() {
                        "empty stderr".to_string()
                    } else {
                        stderr
                    }
                ),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let value = raw
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeLogRecord {
    pub(super) ts_ms: i64,
    pub(super) component: String,
    pub(super) level: String,
    pub(super) message: String,
    pub(super) context_json: Option<String>,
}

enum LogIngestorMessage {
    Log(RuntimeLogRecord),
    Flush(Sender<Result<()>>),
    Shutdown(Sender<Result<()>>),
}

#[derive(Debug)]
pub(super) struct RuntimeLogIngestor {
    sender: Sender<LogIngestorMessage>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RuntimeLogIngestor {
    pub(super) fn start(
        store: NativeRuntimeStateStore,
        owner_token: String,
        config: &RuntimeHardeningConfig,
    ) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<LogIngestorMessage>();
        let batch_size = config.log_batch_size;
        let flush_interval = Duration::from_millis(config.log_flush_interval_ms);
        let retention_days = config.log_retention_days;
        let lease_ttl_ms = config.lease_ttl_ms;
        let retention_interval = Duration::from_secs(config.retention_sweep_interval_secs);
        let thread_owner = owner_token.clone();

        let handle = thread::Builder::new()
            .name("hm-native-log-ingestor".to_string())
            .spawn(move || {
                run_log_ingestor_loop(
                    receiver,
                    store,
                    thread_owner,
                    batch_size,
                    flush_interval,
                    retention_days,
                    lease_ttl_ms,
                    retention_interval,
                );
            })
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_start_failed",
                    format!("Failed to start native runtime log ingestor thread: {error}"),
                )
            })?;

        Ok(Self {
            sender,
            handle: Some(handle),
        })
    }

    pub(super) fn log(&self, log: RuntimeLogRecord) -> Result<()> {
        self.sender
            .send(LogIngestorMessage::Log(log))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to enqueue runtime log for ingestion: {error}"),
                )
            })
    }

    pub(super) fn flush(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(LogIngestorMessage::Flush(tx))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to request runtime log flush: {error}"),
                )
            })?;
        rx.recv().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_log_ingestor_recv_failed",
                format!("Failed to receive runtime log flush result: {error}"),
            )
        })?
    }

    pub(super) fn shutdown(mut self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        self.sender
            .send(LogIngestorMessage::Shutdown(tx))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to request runtime log ingestor shutdown: {error}"),
                )
            })?;
        let shutdown_result = rx.recv().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_log_ingestor_recv_failed",
                format!("Failed to receive runtime log ingestor shutdown result: {error}"),
            )
        })?;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        shutdown_result
    }
}

fn run_log_ingestor_loop(
    receiver: Receiver<LogIngestorMessage>,
    store: NativeRuntimeStateStore,
    owner_token: String,
    batch_size: usize,
    flush_interval: Duration,
    retention_days: u64,
    lease_ttl_ms: u64,
    retention_interval: Duration,
) {
    let mut pending = Vec::<RuntimeLogRecord>::new();
    let mut last_retention = Instant::now();

    loop {
        let message = receiver.recv_timeout(flush_interval);
        match message {
            Ok(LogIngestorMessage::Log(log)) => {
                pending.push(log);
                if pending.len() >= batch_size {
                    let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                }
            }
            Ok(LogIngestorMessage::Flush(reply)) => {
                let result = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = reply.send(result);
            }
            Ok(LogIngestorMessage::Shutdown(reply)) => {
                let result = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = store.release_lease(LOG_INGESTOR_LEASE_NAME, &owner_token);
                let _ = store.release_lease(RETENTION_LEASE_NAME, &owner_token);
                let _ = reply.send(result);
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() {
                    let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = flush_pending_logs(&store, &owner_token, lease_ttl_ms, &mut pending);
                let _ = store.release_lease(LOG_INGESTOR_LEASE_NAME, &owner_token);
                let _ = store.release_lease(RETENTION_LEASE_NAME, &owner_token);
                break;
            }
        }

        if last_retention.elapsed() >= retention_interval {
            if let Ok(true) = store.acquire_lease(RETENTION_LEASE_NAME, &owner_token, lease_ttl_ms)
            {
                let _ = store.cleanup_logs(retention_days);
                let _ = store.heartbeat_lease(RETENTION_LEASE_NAME, &owner_token, lease_ttl_ms);
            }
            last_retention = Instant::now();
        }
    }
}

fn flush_pending_logs(
    store: &NativeRuntimeStateStore,
    owner_token: &str,
    lease_ttl_ms: u64,
    pending: &mut Vec<RuntimeLogRecord>,
) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    store.heartbeat_lease(LOG_INGESTOR_LEASE_NAME, owner_token, lease_ttl_ms)?;
    store.ingest_logs(pending)?;
    pending.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn query_scalar_i64(db_path: &Path, sql: &str) -> i64 {
        let output = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg(db_path)
            .arg(sql)
            .output()
            .expect("sqlite3 query should run");
        assert!(
            output.status.success(),
            "sqlite3 query failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        raw.parse::<i64>().expect("integer sqlite output")
    }

    #[test]
    fn runtime_state_store_applies_wal_and_migrations() {
        let dir = tempdir().expect("temp dir");
        let mut env = HashMap::new();
        env.insert(
            STATE_DB_PATH_ENV.to_string(),
            dir.path()
                .join("native-state.sqlite")
                .to_string_lossy()
                .to_string(),
        );
        let config = RuntimeHardeningConfig::from_env(&env);
        let store = NativeRuntimeStateStore::open(&config).expect("state store should initialize");

        let mode = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg(&store.db_path)
            .arg("PRAGMA journal_mode;")
            .output()
            .expect("sqlite3 pragma query");
        assert!(mode.status.success());
        let journal_mode = String::from_utf8_lossy(&mode.stdout).trim().to_lowercase();
        assert_eq!(journal_mode, "wal");

        let migrations = query_scalar_i64(
            &store.db_path,
            "SELECT COUNT(1) FROM schema_migrations WHERE version IN (1,2);",
        );
        assert_eq!(migrations, 2);
    }

    #[test]
    fn runtime_log_ingestor_batches_and_survives_restart() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().join("state.sqlite");
        let mut env = HashMap::new();
        env.insert(
            STATE_DB_PATH_ENV.to_string(),
            db_path.to_string_lossy().to_string(),
        );
        env.insert(
            SECRETS_STORE_PATH_ENV.to_string(),
            dir.path()
                .join("secrets.enc.json")
                .to_string_lossy()
                .to_string(),
        );
        env.insert(
            SECRETS_KEYRING_PATH_ENV.to_string(),
            dir.path()
                .join("secrets.keyring")
                .to_string_lossy()
                .to_string(),
        );
        env.insert(LOG_BATCH_SIZE_ENV.to_string(), "2".to_string());

        let support = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap should pass");
        support
            .ingest_log("native_bootstrap", "info", "line1", None)
            .expect("log1");
        support
            .ingest_log("native_bootstrap", "info", "line2", None)
            .expect("log2");
        support.flush_logs().expect("flush");
        support.shutdown().expect("shutdown");

        let support_restart = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap restart");
        support_restart.shutdown().expect("shutdown restart");

        let count = query_scalar_i64(&db_path, "SELECT COUNT(1) FROM runtime_logs;");
        assert!(count >= 2, "expected persisted runtime logs across restart");
    }
}
