use super::*;

impl Registry {
    pub(crate) fn timestamp_to_nanos(timestamp: chrono::DateTime<Utc>) -> i64 {
        timestamp
            .timestamp_nanos_opt()
            .unwrap_or_else(|| timestamp.timestamp_micros().saturating_mul(1_000))
    }

    pub(crate) fn sql_text_expr(value: &str) -> String {
        let mut hex = String::with_capacity(value.len().saturating_mul(2));
        for byte in value.as_bytes() {
            let _ = write!(&mut hex, "{byte:02x}");
        }
        format!("CAST(X'{hex}' AS TEXT)")
    }

    pub(crate) fn sql_optional_uuid_expr(value: Option<Uuid>) -> String {
        value.map_or_else(
            || "NULL".to_string(),
            |id| Self::sql_text_expr(&id.to_string()),
        )
    }

    pub(crate) fn sqlite_exec(db_path: &Path, sql: &str, origin: &str) -> Result<()> {
        let mut child = Command::new("sqlite3")
            .arg("-cmd")
            .arg(".timeout 5000")
            .arg(db_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                HivemindError::system(
                    "events_sqlite_exec_failed",
                    format!(
                        "Failed to invoke sqlite3 for '{}': {err}",
                        db_path.display()
                    ),
                    origin,
                )
            })?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(sql.as_bytes()).map_err(|err| {
                HivemindError::system(
                    "events_sqlite_exec_failed",
                    format!(
                        "Failed to send SQL to sqlite3 for '{}': {err}",
                        db_path.display()
                    ),
                    origin,
                )
            })?;
        }

        let output = child.wait_with_output().map_err(|err| {
            HivemindError::system(
                "events_sqlite_exec_failed",
                format!(
                    "Failed to wait for sqlite3 completion for '{}': {err}",
                    db_path.display()
                ),
                origin,
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
        Err(HivemindError::system(
            "events_sqlite_exec_failed",
            format!("sqlite3 failed for '{}': {message}", db_path.display()),
            origin,
        ))
    }

    pub(crate) fn build_sqlite_recovery_sql(events: &[Event], origin: &str) -> Result<String> {
        let mut sql = String::from("BEGIN IMMEDIATE;\n");
        for event in events {
            let sequence = event.metadata.sequence.ok_or_else(|| {
                HivemindError::user(
                    "events_recover_missing_sequence",
                    "Cannot recover event without sequence metadata",
                    origin,
                )
            })?;
            let sequence = i64::try_from(sequence).map_err(|_| {
                HivemindError::system(
                    "events_recover_sequence_out_of_range",
                    "Mirror event sequence exceeds sqlite integer range",
                    origin,
                )
            })?;

            let corr = &event.metadata.correlation;
            let event_json = serde_json::to_string(event).map_err(|err| {
                HivemindError::system(
                    "events_recover_serialize_failed",
                    format!("Failed to serialize event during recovery: {err}"),
                    origin,
                )
            })?;

            let _ = writeln!(
                sql,
                "INSERT INTO events (
                        sequence, event_id, timestamp_nanos, timestamp_rfc3339,
                        project_id, graph_id, flow_id, task_id, attempt_id, event_json
                    ) VALUES (
                        {sequence},
                        {},
                        {},
                        {},
                        {},
                        {},
                        {},
                        {},
                        {},
                        {}
                    );",
                Self::sql_text_expr(&event.id().to_string()),
                Self::timestamp_to_nanos(event.metadata.timestamp),
                Self::sql_text_expr(&event.metadata.timestamp.to_rfc3339()),
                Self::sql_optional_uuid_expr(corr.project_id),
                Self::sql_optional_uuid_expr(corr.graph_id),
                Self::sql_optional_uuid_expr(corr.flow_id),
                Self::sql_optional_uuid_expr(corr.task_id),
                Self::sql_optional_uuid_expr(corr.attempt_id),
                Self::sql_text_expr(&event_json),
            );
        }
        sql.push_str("COMMIT;");
        Ok(sql)
    }

    // Legacy mirror recovery functionality removed
}
