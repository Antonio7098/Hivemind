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

#[derive(Debug, Clone)]
struct NativeRuntimeStateStore {
    db_path: PathBuf,
    busy_timeout_ms: u64,
}

impl NativeRuntimeStateStore {
    fn open(config: &RuntimeHardeningConfig) -> Result<Self> {
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

    fn acquire_lease(&self, job_name: &str, owner_token: &str, ttl_ms: u64) -> Result<bool> {
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
pub struct RuntimeLogRecord {
    pub ts_ms: i64,
    pub component: String,
    pub level: String,
    pub message: String,
    pub context_json: Option<String>,
}

enum LogIngestorMessage {
    Log(RuntimeLogRecord),
    Flush(Sender<Result<()>>),
    Shutdown(Sender<Result<()>>),
}

#[derive(Debug)]
struct RuntimeLogIngestor {
    sender: Sender<LogIngestorMessage>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RuntimeLogIngestor {
    fn start(
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

    fn log(&self, log: RuntimeLogRecord) -> Result<()> {
        self.sender
            .send(LogIngestorMessage::Log(log))
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_log_ingestor_send_failed",
                    format!("Failed to enqueue runtime log for ingestion: {error}"),
                )
            })
    }

    fn flush(&self) -> Result<()> {
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

    fn shutdown(mut self) -> Result<()> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedSecretsFile {
    version: u32,
    nonce_hex: String,
    ciphertext_hex: String,
    tag_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PlainSecretsFile {
    version: u32,
    secrets: BTreeMap<String, String>,
}

impl Default for PlainSecretsFile {
    fn default() -> Self {
        Self {
            version: 1,
            secrets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct NativeSecretsManager {
    store_path: PathBuf,
    keyring_path: PathBuf,
    env_master_key_b64: Option<String>,
}

impl NativeSecretsManager {
    fn open(config: &RuntimeHardeningConfig, env: &HashMap<String, String>) -> Result<Self> {
        let env_master_key_b64 = env
            .get(SECRETS_MASTER_KEY_ENV)
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let manager = Self {
            store_path: config.secrets_store_path.clone(),
            keyring_path: config.secrets_keyring_path.clone(),
            env_master_key_b64,
        };

        if let Some(parent) = manager.store_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_store_dir_create_failed",
                    format!(
                        "Failed to create native secrets store dir '{}': {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        if let Some(parent) = manager.keyring_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_keyring_dir_create_failed",
                    format!(
                        "Failed to create native secrets keyring dir '{}': {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        Ok(manager)
    }

    fn set_secret(&self, name: &str, value: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(RuntimeHardeningError::new(
                "native_secret_invalid_name",
                "Secret name must not be empty",
            ));
        }
        if value.trim().is_empty() {
            return Err(RuntimeHardeningError::new(
                "native_secret_invalid_value",
                "Secret value must not be empty",
            ));
        }

        let mut file = self.load_plain_file()?;
        file.secrets.insert(name.to_string(), value.to_string());
        self.persist_plain_file(&file)
    }

    fn get_secret(&self, name: &str) -> Result<Option<String>> {
        let file = self.load_plain_file()?;
        Ok(file.secrets.get(name).cloned())
    }

    fn load_plain_file(&self) -> Result<PlainSecretsFile> {
        if !self.store_path.exists() {
            return Ok(PlainSecretsFile::default());
        }
        let encrypted_raw = fs::read(&self.store_path).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_store_read_failed",
                format!(
                    "Failed to read native secrets store '{}': {error}",
                    self.store_path.display()
                ),
            )
        })?;
        let envelope =
            serde_json::from_slice::<EncryptedSecretsFile>(&encrypted_raw).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_store_decode_failed",
                    format!(
                        "Failed to decode encrypted native secrets store '{}': {error}",
                        self.store_path.display()
                    ),
                )
            })?;

        let key = self.load_or_create_master_key()?;
        let nonce = decode_hex(&envelope.nonce_hex, "native_secrets_nonce_decode_failed")?;
        let ciphertext = decode_hex(
            &envelope.ciphertext_hex,
            "native_secrets_ciphertext_decode_failed",
        )?;
        let expected_tag = decode_hex(&envelope.tag_hex, "native_secrets_tag_decode_failed")?;
        let actual_tag = compute_cipher_tag(&key, &nonce, &ciphertext);
        if expected_tag != actual_tag.as_slice() {
            return Err(RuntimeHardeningError::new(
                "native_secrets_decrypt_failed",
                "Native secrets ciphertext failed integrity verification",
            ));
        }
        let mut plaintext = xor_stream_cipher(&key, &nonce, &ciphertext);

        let parsed = serde_json::from_slice::<PlainSecretsFile>(&plaintext).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_plaintext_decode_failed",
                format!("Failed to decode native secrets plaintext: {error}"),
            )
        })?;
        wipe_buffer(&mut plaintext);
        Ok(parsed)
    }

    fn persist_plain_file(&self, file: &PlainSecretsFile) -> Result<()> {
        let mut plaintext = serde_json::to_vec(file).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_plaintext_encode_failed",
                format!("Failed to encode native secrets plaintext: {error}"),
            )
        })?;

        let key = self.load_or_create_master_key()?;
        let nonce_bytes = generate_nonce();
        let ciphertext = xor_stream_cipher(&key, &nonce_bytes, &plaintext);
        let tag = compute_cipher_tag(&key, &nonce_bytes, &ciphertext);

        let envelope = EncryptedSecretsFile {
            version: 1,
            nonce_hex: encode_hex(&nonce_bytes),
            ciphertext_hex: encode_hex(&ciphertext),
            tag_hex: encode_hex(&tag),
        };
        let serialized = serde_json::to_vec_pretty(&envelope).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_store_encode_failed",
                format!("Failed to encode encrypted native secrets store: {error}"),
            )
        })?;
        atomic_write(&self.store_path, &serialized)?;
        wipe_buffer(&mut plaintext);
        Ok(())
    }

    fn load_or_create_master_key(&self) -> Result<[u8; 32]> {
        if let Some(raw) = self.env_master_key_b64.as_deref() {
            let decoded = decode_hex(raw, "native_secrets_master_key_decode_failed")?;
            if decoded.len() != 32 {
                return Err(RuntimeHardeningError::new(
                    "native_secrets_master_key_invalid",
                    "HIVEMIND_NATIVE_SECRETS_MASTER_KEY must decode to 32 hex bytes",
                ));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return Ok(key);
        }

        if self.keyring_path.exists() {
            let encoded = fs::read_to_string(&self.keyring_path).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_keyring_read_failed",
                    format!(
                        "Failed to read native secrets keyring '{}': {error}",
                        self.keyring_path.display()
                    ),
                )
            })?;
            let decoded = decode_hex(encoded.trim(), "native_secrets_master_key_decode_failed")?;
            if decoded.len() != 32 {
                return Err(RuntimeHardeningError::new(
                    "native_secrets_master_key_invalid",
                    format!(
                        "Native secrets keyring '{}' contains invalid key length",
                        self.keyring_path.display()
                    ),
                ));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return Ok(key);
        }

        let key = generate_master_key();
        let encoded = encode_hex(&key);
        atomic_write(&self.keyring_path, encoded.as_bytes())?;
        Ok(key)
    }
}

fn generate_master_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(now_ms().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest[..32]);
    key
}

fn generate_nonce() -> [u8; 24] {
    let mut hasher = Sha256::new();
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(now_ms().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let digest = hasher.finalize();
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&digest[..24]);
    nonce
}

fn xor_stream_cipher(key: &[u8; 32], nonce: &[u8], input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut offset = 0usize;
    let mut counter = 0u64;
    while offset < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(nonce);
        hasher.update(counter.to_le_bytes());
        let block = hasher.finalize();
        let block_len = (input.len() - offset).min(block.len());
        for idx in 0..block_len {
            output.push(input[offset + idx] ^ block[idx]);
        }
        offset += block_len;
        counter = counter.saturating_add(1);
    }
    output
}

fn compute_cipher_tag(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..32]);
    out
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(raw: &str, code: &str) -> Result<Vec<u8>> {
    if raw.len() % 2 != 0 {
        return Err(RuntimeHardeningError::new(
            code,
            format!("Invalid hex length {}", raw.len()),
        ));
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let high = decode_hex_nibble(pair[0]).ok_or_else(|| {
            RuntimeHardeningError::new(code, format!("Invalid hex digit '{}'", pair[0] as char))
        })?;
        let low = decode_hex_nibble(pair[1]).ok_or_else(|| {
            RuntimeHardeningError::new(code, format!("Invalid hex digit '{}'", pair[1] as char))
        })?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn wipe_buffer(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        *byte = 0;
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!("Path '{}' does not have a parent directory", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to create parent dir '{}': {error}",
                parent.display()
            ),
        )
    })?;
    let tmp_path = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("native"),
        Uuid::new_v4()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut tmp_file = options.open(&tmp_path).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to create temp file '{}': {error}",
                tmp_path.display()
            ),
        )
    })?;
    tmp_file.write_all(bytes).map_err(|error| {
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to write temp file '{}': {error}",
                tmp_path.display()
            ),
        )
    })?;
    drop(tmp_file);
    fs::rename(&tmp_path, path).map_err(|error| {
        let _ = fs::remove_file(&tmp_path);
        RuntimeHardeningError::new(
            "native_atomic_write_failed",
            format!(
                "Failed to atomically replace '{}' with '{}': {error}",
                path.display(),
                tmp_path.display()
            ),
        )
    })?;
    Ok(())
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

    #[test]
    fn secrets_manager_encrypts_with_atomic_store() {
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
        let mut runtime_env = HashMap::new();
        runtime_env.insert("OPENROUTER_API_KEY".to_string(), "s3cr3t".to_string());
        support
            .ensure_secret_from_or_to_env(&mut runtime_env, "OPENROUTER_API_KEY")
            .expect("secret import");
        support.shutdown().expect("shutdown");

        let raw = fs::read_to_string(dir.path().join("secrets.enc.json")).expect("encrypted store");
        assert!(
            !raw.contains("s3cr3t"),
            "encrypted store must not contain plaintext secret"
        );

        let support_restart = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap restart");
        let mut runtime_env2 = HashMap::new();
        support_restart
            .ensure_secret_from_or_to_env(&mut runtime_env2, "OPENROUTER_API_KEY")
            .expect("secret export");
        assert_eq!(
            runtime_env2.get("OPENROUTER_API_KEY"),
            Some(&"s3cr3t".to_string())
        );
        support_restart.shutdown().expect("shutdown restart");
    }

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

    #[test]
    #[cfg(unix)]
    fn atomic_write_restricts_secret_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("secret.keyring");
        atomic_write(&path, b"secret").expect("atomic write should pass");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
