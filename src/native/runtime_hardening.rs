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

mod config;
mod readiness;
mod secrets;
mod storage;
mod support;

#[cfg(test)]
mod tests;

use self::readiness::NativeReadinessGate;
use self::secrets::NativeSecretsManager;
#[allow(unused_imports)]
pub(crate) use self::storage::{
    GraphCodeArtifactRecord, GraphCodeArtifactUpsert, GraphCodeSessionUpsert,
    NativeRuntimeStateStore,
};
use self::storage::{RuntimeLogIngestor, RuntimeLogRecord};
pub(crate) use self::support::cleanup_native_blob_storage;

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
const DEFAULT_BLOB_RETENTION_DAYS: u64 = 30;
const DEFAULT_LEASE_TTL_MS: u64 = 30_000;
const DEFAULT_RETENTION_SWEEP_INTERVAL_SECS: u64 = 30;

pub(crate) const STATE_DB_PATH_ENV: &str = "HIVEMIND_NATIVE_STATE_DB_PATH";
pub(crate) const STATE_DIR_ENV: &str = "HIVEMIND_NATIVE_STATE_DIR";
const BUSY_TIMEOUT_ENV: &str = "HIVEMIND_NATIVE_STATE_BUSY_TIMEOUT_MS";
const LOG_BATCH_SIZE_ENV: &str = "HIVEMIND_NATIVE_LOG_BATCH_SIZE";
const LOG_FLUSH_INTERVAL_ENV: &str = "HIVEMIND_NATIVE_LOG_FLUSH_INTERVAL_MS";
const LOG_RETENTION_DAYS_ENV: &str = "HIVEMIND_NATIVE_LOG_RETENTION_DAYS";
const BLOB_RETENTION_DAYS_ENV: &str = "HIVEMIND_NATIVE_BLOB_RETENTION_DAYS";
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
    pub blob_storage_dir: PathBuf,
    pub busy_timeout_ms: u64,
    pub log_batch_size: usize,
    pub log_flush_interval_ms: u64,
    pub log_retention_days: u64,
    pub blob_retention_days: u64,
    pub lease_ttl_ms: u64,
    pub retention_sweep_interval_secs: u64,
    pub secrets_store_path: PathBuf,
    pub secrets_keyring_path: PathBuf,
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

#[derive(Debug)]
pub struct NativeRuntimeSupport {
    log_ingestor: RuntimeLogIngestor,
    secrets: NativeSecretsManager,
    readiness: NativeReadinessGate,
    telemetry: NativeRuntimeStateTelemetry,
}
