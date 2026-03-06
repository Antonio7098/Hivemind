use super::*;
use fs2::FileExt;
use std::fmt::Write;
use std::process::Command;

/// SQLite-backed event store (canonical) with optional JSONL mirror for inspectability.
#[derive(Debug)]
pub struct SqliteEventStore {
    db: PathBuf,
    legacy_mirror: PathBuf,
    write_lock: PathBuf,
}

impl SqliteEventStore {
    /// Creates or opens a SQLite-backed event store rooted at `base_dir`.
    pub fn open(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;

        let db = base_dir.join("db.sqlite");
        let legacy_mirror = base_dir.join("events.jsonl");
        let write_lock = base_dir.join("db.write.lock");
        Self::run_sql_batch_on_path(
            &db,
            r"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;

            CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY NOT NULL,
                event_id TEXT NOT NULL UNIQUE,
                timestamp_nanos INTEGER NOT NULL,
                timestamp_rfc3339 TEXT NOT NULL,
                project_id TEXT NULL,
                graph_id TEXT NULL,
                flow_id TEXT NULL,
                task_id TEXT NULL,
                attempt_id TEXT NULL,
                event_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_events_project_id ON events(project_id);
            CREATE INDEX IF NOT EXISTS idx_events_graph_id ON events(graph_id);
            CREATE INDEX IF NOT EXISTS idx_events_flow_id ON events(flow_id);
            CREATE INDEX IF NOT EXISTS idx_events_task_id ON events(task_id);
            CREATE INDEX IF NOT EXISTS idx_events_attempt_id ON events(attempt_id);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp_nanos ON events(timestamp_nanos);

            CREATE TRIGGER IF NOT EXISTS trg_events_append_only_update
            BEFORE UPDATE ON events
            BEGIN
                SELECT RAISE(ABORT, 'events_append_only_violation');
            END;

            CREATE TRIGGER IF NOT EXISTS trg_events_append_only_delete
            BEFORE DELETE ON events
            BEGIN
                SELECT RAISE(ABORT, 'events_append_only_violation');
            END;
            ",
        )?;

        Ok(Self {
            db,
            legacy_mirror,
            write_lock,
        })
    }

    fn sqlite_output_with_retry(
        db_path: &Path,
        sql: &str,
        json_output: bool,
    ) -> Result<std::process::Output> {
        const MAX_LOCK_RETRIES: usize = 50;
        let mut attempts = 0usize;
        loop {
            let mut cmd = Command::new("sqlite3");
            if json_output {
                cmd.arg("-json");
            }
            let output = cmd
                .arg("-cmd")
                .arg(".timeout 5000")
                .arg(db_path)
                .arg(sql)
                .output()?;

            if output.status.success() {
                return Ok(output);
            }

            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            if stderr.contains("database is locked") && attempts < MAX_LOCK_RETRIES {
                attempts = attempts.saturating_add(1);
                thread::sleep(Duration::from_millis(20));
                continue;
            }

            return Err(EventStoreError::Invariant(stderr.trim().to_string()));
        }
    }

    pub(super) fn run_sql_batch_on_path(db_path: &Path, sql: &str) -> Result<()> {
        let _ = Self::sqlite_output_with_retry(db_path, sql, false)?;
        Ok(())
    }

    fn run_sql_json_query_on_path(db_path: &Path, sql: &str) -> Result<Vec<serde_json::Value>> {
        let output = Self::sqlite_output_with_retry(db_path, sql, true)?;
        if output.stdout.is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice::<Vec<serde_json::Value>>(
            &output.stdout,
        )?)
    }

    fn run_sql_batch(&self, sql: &str) -> Result<()> {
        Self::run_sql_batch_on_path(&self.db, sql)
    }

    fn run_sql_json_query(&self, sql: &str) -> Result<Vec<serde_json::Value>> {
        Self::run_sql_json_query_on_path(&self.db, sql)
    }

    fn sql_text_expr(value: &str) -> String {
        let mut hex = String::with_capacity(value.len().saturating_mul(2));
        for byte in value.as_bytes() {
            let _ = write!(&mut hex, "{byte:02x}");
        }
        format!("CAST(X'{hex}' AS TEXT)")
    }

    fn sql_optional_uuid_expr(value: Option<Uuid>) -> String {
        value.map_or_else(
            || "NULL".to_string(),
            |id| Self::sql_text_expr(&id.to_string()),
        )
    }

    fn parse_i64_field(row: &serde_json::Value, key: &str) -> Result<i64> {
        if let Some(value) = row.get(key).and_then(serde_json::Value::as_i64) {
            return Ok(value);
        }
        if let Some(value) = row.get(key).and_then(serde_json::Value::as_str) {
            return value.parse::<i64>().map_err(|_| {
                EventStoreError::Invariant(format!("invalid integer field '{key}' in sqlite row"))
            });
        }
        Err(EventStoreError::Invariant(format!(
            "missing integer field '{key}' in sqlite row"
        )))
    }

    fn parse_event_field(row: &serde_json::Value) -> Result<Event> {
        let raw = row
            .get("event_json")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                EventStoreError::Invariant("sqlite row missing event_json field".to_string())
            })?;
        Ok(serde_json::from_str::<Event>(raw)?)
    }

    fn append_legacy_mirror(&self, event_json: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        if let Some(parent) = self.legacy_mirror.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&self.legacy_mirror)?;
        file.lock_exclusive()?;
        writeln!(file, "{event_json}")?;
        let _ = file.flush();
        let _ = file.unlock();
        Ok(())
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for SqliteEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;

        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&self.write_lock)?;
        lock_file.lock_exclusive()?;

        let rows = self.run_sql_json_query(
            "SELECT sequence, timestamp_nanos FROM events ORDER BY sequence DESC LIMIT 1;",
        )?;
        let last_row = rows.first();
        let last_seq = last_row
            .map(|row| Self::parse_i64_field(row, "sequence"))
            .transpose()?;
        let last_nanos = last_row
            .map(|row| Self::parse_i64_field(row, "timestamp_nanos"))
            .transpose()?;

        let next_seq = if let Some(seq) = last_seq {
            let seq_u64 = u64::try_from(seq).map_err(|_| {
                EventStoreError::Invariant("event sequence out of range".to_string())
            })?;
            seq_u64.saturating_add(1)
        } else {
            0
        };
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);

        if let Some(last_nanos) = last_nanos {
            let mut event_nanos = timestamp_to_nanos(event.metadata.timestamp);
            if event_nanos <= last_nanos {
                event_nanos = last_nanos.saturating_add(1);
                event.metadata.timestamp = nanos_to_timestamp(event_nanos);
            }
        }

        let event_json = serde_json::to_string(&event)?;
        let corr = &event.metadata.correlation;
        let seq_i64 = i64::try_from(next_seq).map_err(|_| {
            EventStoreError::Invariant("event sequence exceeds sqlite integer range".to_string())
        })?;
        let insert_sql = format!(
            "INSERT INTO events (
                sequence, event_id, timestamp_nanos, timestamp_rfc3339,
                project_id, graph_id, flow_id, task_id, attempt_id, event_json
            ) VALUES (
                {seq_i64},
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
            timestamp_to_nanos(event.metadata.timestamp),
            Self::sql_text_expr(&event.metadata.timestamp.to_rfc3339()),
            Self::sql_optional_uuid_expr(corr.project_id),
            Self::sql_optional_uuid_expr(corr.graph_id),
            Self::sql_optional_uuid_expr(corr.flow_id),
            Self::sql_optional_uuid_expr(corr.task_id),
            Self::sql_optional_uuid_expr(corr.attempt_id),
            Self::sql_text_expr(&event_json),
        );

        let result = self.run_sql_batch(&insert_sql);
        let _ = lock_file.unlock();
        result?;
        self.append_legacy_mirror(&event_json)?;
        Ok(event.id())
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let mut query = String::from("SELECT event_json FROM events");
        let mut conditions = Vec::new();

        if let Some(project_id) = filter.project_id {
            conditions.push(format!(
                "project_id = {}",
                Self::sql_text_expr(&project_id.to_string())
            ));
        }
        if let Some(graph_id) = filter.graph_id {
            conditions.push(format!(
                "graph_id = {}",
                Self::sql_text_expr(&graph_id.to_string())
            ));
        }
        if let Some(flow_id) = filter.flow_id {
            conditions.push(format!(
                "flow_id = {}",
                Self::sql_text_expr(&flow_id.to_string())
            ));
        }
        if let Some(task_id) = filter.task_id {
            conditions.push(format!(
                "task_id = {}",
                Self::sql_text_expr(&task_id.to_string())
            ));
        }
        if let Some(attempt_id) = filter.attempt_id {
            conditions.push(format!(
                "attempt_id = {}",
                Self::sql_text_expr(&attempt_id.to_string())
            ));
        }
        if let Some(since) = filter.since {
            conditions.push(format!("timestamp_nanos >= {}", timestamp_to_nanos(since)));
        }
        if let Some(until) = filter.until {
            conditions.push(format!("timestamp_nanos <= {}", timestamp_to_nanos(until)));
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }
        query.push_str(" ORDER BY sequence ASC;");

        let rows = self.run_sql_json_query(&query)?;
        let mut result = Vec::new();
        for row in rows {
            let event = Self::parse_event_field(&row)?;
            if !filter.matches(&event) {
                continue;
            }
            result.push(event);
            if let Some(limit) = filter.limit {
                if result.len() >= limit {
                    break;
                }
            }
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let rows =
            self.run_sql_json_query("SELECT event_json FROM events ORDER BY sequence ASC;")?;
        let mut events = Vec::new();
        for row in rows {
            events.push(Self::parse_event_field(&row)?);
        }
        Ok(events)
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let db_path = self.db.clone();

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen_sequence = -1_i64;

            loop {
                let query = format!(
                    "SELECT sequence, event_json FROM events WHERE sequence > {seen_sequence} ORDER BY sequence ASC;"
                );
                let Ok(rows) = Self::run_sql_json_query_on_path(&db_path, &query) else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                for row in rows {
                    let sequence = Self::parse_i64_field(&row, "sequence");
                    let raw = row
                        .get("event_json")
                        .and_then(serde_json::Value::as_str)
                        .map(std::string::ToString::to_string);
                    let (Ok(sequence), Some(raw)) = (sequence, raw) else {
                        continue;
                    };
                    seen_sequence = sequence;

                    let Ok(event) = serde_json::from_str::<Event>(&raw) else {
                        continue;
                    };
                    if !filter.matches(&event) {
                        continue;
                    }
                    if tx.send(event).is_err() {
                        return;
                    }

                    sent += 1;
                    if let Some(limit) = filter.limit {
                        if sent >= limit {
                            return;
                        }
                    }
                }

                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}
