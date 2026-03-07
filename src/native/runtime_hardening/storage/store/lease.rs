use super::*;

impl NativeRuntimeStateStore {
    pub(crate) fn acquire_lease(
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

    pub(crate) fn heartbeat_lease(
        &self,
        job_name: &str,
        owner_token: &str,
        ttl_ms: u64,
    ) -> Result<()> {
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

    pub(crate) fn release_lease(&self, job_name: &str, owner_token: &str) -> Result<()> {
        let sql = format!(
            "PRAGMA busy_timeout={};\
             DELETE FROM job_leases WHERE job_name='{}' AND owner_token='{}';",
            self.busy_timeout_ms,
            sql_escape(job_name),
            sql_escape(owner_token),
        );
        self.exec(&sql)
    }
}
