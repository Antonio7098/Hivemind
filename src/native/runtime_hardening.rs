//! Native runtime hardening services for Sprint 57.
//!
//! This module provides:
//! - durable native runtime state storage (SQLite with WAL/migrations)
//! - lease/heartbeat ownership for background jobs
//! - batched asynchronous runtime log ingestion with retention cleanup
//! - encrypted local secrets storage with key material in a keyring file
//! - tokenized readiness gating for runtime startup sequencing
#![allow(
    clippy::doc_markdown,
    clippy::equatable_if_let,
    clippy::format_push_string,
    clippy::items_after_statements,
    clippy::manual_is_multiple_of,
    clippy::map_unwrap_or,
    clippy::needless_pass_by_value,
    clippy::redundant_clone,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

mod secrets;
mod storage;

use self::secrets::NativeSecretsManager;
use self::storage::{NativeRuntimeStateStore, RuntimeLogIngestor, RuntimeLogRecord};

use crate::adapters::runtime::{
    NativeReadinessTransition, NativeRuntimeStateTelemetry, RuntimeError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const DEFAULT_BUSY_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_LOG_BATCH_SIZE: usize = 32;
const DEFAULT_LOG_FLUSH_INTERVAL_MS: u64 = 150;
const DEFAULT_LOG_RETENTION_DAYS: u64 = 14;
const DEFAULT_LEASE_TTL_MS: u64 = 30_000;
const DEFAULT_RETENTION_SWEEP_INTERVAL_SECS: u64 = 30;

const STATE_DB_PATH_ENV: &str = "HIVEMIND_NATIVE_STATE_DB_PATH";
const STATE_DIR_ENV: &str = "HIVEMIND_NATIVE_STATE_DIR";
const BUSY_TIMEOUT_ENV: &str = "HIVEMIND_NATIVE_STATE_BUSY_TIMEOUT_MS";
const LOG_BATCH_SIZE_ENV: &str = "HIVEMIND_NATIVE_LOG_BATCH_SIZE";
const LOG_FLUSH_INTERVAL_ENV: &str = "HIVEMIND_NATIVE_LOG_FLUSH_INTERVAL_MS";
const LOG_RETENTION_DAYS_ENV: &str = "HIVEMIND_NATIVE_LOG_RETENTION_DAYS";
const LEASE_TTL_ENV: &str = "HIVEMIND_NATIVE_LEASE_TTL_MS";
const RETENTION_SWEEP_INTERVAL_ENV: &str = "HIVEMIND_NATIVE_RETENTION_SWEEP_INTERVAL_SECS";

const SECRETS_STORE_PATH_ENV: &str = "HIVEMIND_NATIVE_SECRETS_STORE_PATH";
const SECRETS_KEYRING_PATH_ENV: &str = "HIVEMIND_NATIVE_SECRETS_KEYRING_PATH";
const SECRETS_MASTER_KEY_ENV: &str = "HIVEMIND_NATIVE_SECRETS_MASTER_KEY";

const LOG_INGESTOR_LEASE_NAME: &str = "native_runtime_log_ingestor";
const RETENTION_LEASE_NAME: &str = "native_runtime_log_retention";

#[derive(Debug, Clone)]
pub struct RuntimeHardeningError {
    pub code: String,
    pub message: String,
}

impl RuntimeHardeningError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    #[must_use]
    pub fn to_runtime_error(&self) -> RuntimeError {
        RuntimeError::new(self.code.clone(), self.message.clone(), false)
    }
}

type Result<T> = std::result::Result<T, RuntimeHardeningError>;

#[derive(Debug, Clone)]
pub struct RuntimeHardeningConfig {
    pub state_db_path: PathBuf,
    pub busy_timeout_ms: u64,
    pub log_batch_size: usize,
    pub log_flush_interval_ms: u64,
    pub log_retention_days: u64,
    pub lease_ttl_ms: u64,
    pub retention_sweep_interval_secs: u64,
    pub secrets_store_path: PathBuf,
    pub secrets_keyring_path: PathBuf,
}

impl RuntimeHardeningConfig {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let state_dir = env
            .get(STATE_DIR_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env.get("HIVEMIND_DATA_DIR")
                    .filter(|value| !value.trim().is_empty())
                    .map(PathBuf::from)
            })
            .or_else(default_hivemind_data_dir)
            .unwrap_or_else(|| PathBuf::from(".hivemind"));

        let state_db_path = env
            .get(STATE_DB_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("runtime-state.sqlite"));
        let secrets_store_path = env
            .get(SECRETS_STORE_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("secrets.enc.json"));
        let secrets_keyring_path = env
            .get(SECRETS_KEYRING_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("secrets.keyring"));

        Self {
            state_db_path,
            busy_timeout_ms: parse_u64_env(
                env,
                BUSY_TIMEOUT_ENV,
                DEFAULT_BUSY_TIMEOUT_MS,
                1,
                60_000,
            ),
            log_batch_size: parse_usize_env(
                env,
                LOG_BATCH_SIZE_ENV,
                DEFAULT_LOG_BATCH_SIZE,
                1,
                1024,
            ),
            log_flush_interval_ms: parse_u64_env(
                env,
                LOG_FLUSH_INTERVAL_ENV,
                DEFAULT_LOG_FLUSH_INTERVAL_MS,
                10,
                10_000,
            ),
            log_retention_days: parse_u64_env(
                env,
                LOG_RETENTION_DAYS_ENV,
                DEFAULT_LOG_RETENTION_DAYS,
                1,
                365,
            ),
            lease_ttl_ms: parse_u64_env(env, LEASE_TTL_ENV, DEFAULT_LEASE_TTL_MS, 1_000, 300_000),
            retention_sweep_interval_secs: parse_u64_env(
                env,
                RETENTION_SWEEP_INTERVAL_ENV,
                DEFAULT_RETENTION_SWEEP_INTERVAL_SECS,
                5,
                3600,
            ),
            secrets_store_path,
            secrets_keyring_path,
        }
    }
}

fn parse_u64_env(
    env: &HashMap<String, String>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

fn parse_usize_env(
    env: &HashMap<String, String>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> usize {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

fn default_hivemind_data_dir() -> Option<PathBuf> {
    std::env::var("HIVEMIND_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".hivemind")))
}

fn now_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn sql_escape(raw: &str) -> String {
    raw.replace('\'', "''")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadinessState {
    Pending,
    Ready,
    Failed,
}

impl ReadinessState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
struct ReadinessEntry {
    token: String,
    state: ReadinessState,
}

#[derive(Debug, Clone)]
struct NativeReadinessGate {
    components: BTreeMap<String, ReadinessEntry>,
    transitions: Vec<NativeReadinessTransition>,
}

impl NativeReadinessGate {
    fn new() -> Self {
        Self {
            components: BTreeMap::new(),
            transitions: Vec::new(),
        }
    }

    fn register(&mut self, component: &str) -> String {
        let token = format!("{component}:{}", Uuid::new_v4());
        self.components.insert(
            component.to_string(),
            ReadinessEntry {
                token: token.clone(),
                state: ReadinessState::Pending,
            },
        );
        token
    }

    fn mark_ready(&mut self, token: &str, reason: impl Into<String>) {
        self.transition(token, ReadinessState::Ready, reason.into());
    }

    fn mark_failed(&mut self, token: &str, reason: impl Into<String>) {
        self.transition(token, ReadinessState::Failed, reason.into());
    }

    fn transition(&mut self, token: &str, target: ReadinessState, reason: String) {
        let Some((component_name, entry)) = self
            .components
            .iter_mut()
            .find(|(_, candidate)| candidate.token == token)
        else {
            return;
        };
        let from_state = entry.state;
        entry.state = target;
        self.transitions.push(NativeReadinessTransition {
            component: component_name.clone(),
            token: token.to_string(),
            from_state: from_state.as_str().to_string(),
            to_state: target.as_str().to_string(),
            reason,
            timestamp_ms: now_ms(),
        });
    }

    fn all_ready(&self) -> bool {
        self.components
            .values()
            .all(|entry| entry.state == ReadinessState::Ready)
    }

    fn transitions(&self) -> Vec<NativeReadinessTransition> {
        self.transitions.clone()
    }
}

#[derive(Debug)]
pub struct NativeRuntimeSupport {
    log_ingestor: RuntimeLogIngestor,
    secrets: NativeSecretsManager,
    readiness: NativeReadinessGate,
    telemetry: NativeRuntimeStateTelemetry,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn readiness_transitions_are_recorded_for_all_components() {
        let dir = tempdir().expect("temp dir");
        let mut env = HashMap::new();
        env.insert(
            STATE_DB_PATH_ENV.to_string(),
            dir.path()
                .join("state.sqlite")
                .to_string_lossy()
                .to_string(),
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

        let support = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap should pass");
        let transitions = support.readiness_transitions();
        assert!(transitions
            .iter()
            .any(|transition| transition.component == "runtime_state_db"
                && transition.to_state == "ready"));
        assert!(transitions
            .iter()
            .any(|transition| transition.component == "secrets_manager"
                && transition.to_state == "ready"));
        assert!(transitions
            .iter()
            .any(|transition| transition.component == "runtime_log_ingestor"
                && transition.to_state == "ready"));
        support.shutdown().expect("shutdown");
    }
}
