use super::*;

impl NativeRuntimeStateStore {
    pub(crate) fn open(config: &RuntimeHardeningConfig) -> Result<Self> {
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
}
