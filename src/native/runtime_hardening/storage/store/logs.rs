use super::*;

impl NativeRuntimeStateStore {
    pub(crate) fn ingest_logs(&self, logs: &[RuntimeLogRecord]) -> Result<()> {
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

    pub(crate) fn cleanup_logs(&self, retention_days: u64) -> Result<u64> {
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
}
