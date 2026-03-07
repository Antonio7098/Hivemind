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
