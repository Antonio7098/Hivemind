//! `EventStore` trait and implementations.
//!
//! Event stores are the persistence layer for events. All state is derived
//! from events, so the event store is the single source of truth.

use crate::core::events::{Event, EventId, EventPayload};
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

fn normalize_concatenated_json_objects(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut escape = false;

    while let Some(c) = chars.next() {
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        }

        out.push(c);

        if !in_string && c == '}' && chars.peek().copied() == Some('{') {
            out.push('\n');
        }
    }

    out
}

fn timestamp_to_nanos(timestamp: DateTime<Utc>) -> i64 {
    timestamp
        .timestamp_nanos_opt()
        .unwrap_or_else(|| timestamp.timestamp_micros().saturating_mul(1_000))
}

fn nanos_to_timestamp(nanos: i64) -> DateTime<Utc> {
    let seconds = nanos.div_euclid(1_000_000_000);
    let subsec_nanos = u32::try_from(nanos.rem_euclid(1_000_000_000)).unwrap_or(0);
    DateTime::<Utc>::from_timestamp(seconds, subsec_nanos).unwrap_or_else(Utc::now)
}

/// Errors that can occur in the event store.
#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Invariant violation: {0}")]
    Invariant(String),
    #[error("Event not found: {0}")]
    NotFound(EventId),
}

/// Result type for event store operations.
pub type Result<T> = std::result::Result<T, EventStoreError>;

/// Filter for querying events.
#[derive(Debug, Default, Clone)]
pub struct EventFilter {
    /// Filter by project ID.
    pub project_id: Option<Uuid>,
    /// Filter by graph ID.
    pub graph_id: Option<Uuid>,
    /// Filter by task ID.
    pub task_id: Option<Uuid>,
    /// Filter by flow ID.
    pub flow_id: Option<Uuid>,
    /// Filter by attempt ID.
    pub attempt_id: Option<Uuid>,
    /// Filter by governance artifact ID/key.
    pub artifact_id: Option<String>,
    /// Filter by template ID.
    pub template_id: Option<String>,
    /// Filter by constitution rule ID.
    pub rule_id: Option<String>,
    /// Include events at or after this timestamp.
    pub since: Option<DateTime<Utc>>,
    /// Include events at or before this timestamp.
    pub until: Option<DateTime<Utc>>,
    /// Maximum number of events to return.
    pub limit: Option<usize>,
}

impl EventFilter {
    /// Creates an empty filter (matches all events).
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// Filter by project ID.
    #[must_use]
    pub fn for_project(project_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn for_graph(graph_id: Uuid) -> Self {
        Self {
            graph_id: Some(graph_id),
            ..Default::default()
        }
    }

    /// Checks if an event matches this filter.
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        if let Some(pid) = self.project_id {
            if event.metadata.correlation.project_id != Some(pid) {
                return false;
            }
        }
        if let Some(gid) = self.graph_id {
            if event.metadata.correlation.graph_id != Some(gid) {
                return false;
            }
        }
        if let Some(tid) = self.task_id {
            if event.metadata.correlation.task_id != Some(tid) {
                return false;
            }
        }
        if let Some(fid) = self.flow_id {
            if event.metadata.correlation.flow_id != Some(fid) {
                return false;
            }
        }
        if let Some(aid) = self.attempt_id {
            if event.metadata.correlation.attempt_id != Some(aid) {
                return false;
            }
        }
        if let Some(artifact_id) = self.artifact_id.as_deref() {
            if !Self::matches_artifact_id(&event.payload, artifact_id) {
                return false;
            }
        }
        if let Some(template_id) = self.template_id.as_deref() {
            if !Self::matches_template_id(&event.payload, template_id) {
                return false;
            }
        }
        if let Some(rule_id) = self.rule_id.as_deref() {
            if !Self::matches_rule_id(&event.payload, rule_id) {
                return false;
            }
        }
        if let Some(since) = self.since {
            if event.metadata.timestamp < since {
                return false;
            }
        }
        if let Some(until) = self.until {
            if event.metadata.timestamp > until {
                return false;
            }
        }
        true
    }

    fn matches_artifact_id(payload: &EventPayload, artifact_id: &str) -> bool {
        match payload {
            EventPayload::GovernanceArtifactUpserted { artifact_key, .. }
            | EventPayload::GovernanceArtifactDeleted { artifact_key, .. }
            | EventPayload::GovernanceAttachmentLifecycleUpdated { artifact_key, .. } => {
                artifact_key == artifact_id
            }
            EventPayload::TemplateInstantiated {
                template_id,
                system_prompt_id,
                skill_ids,
                document_ids,
                ..
            } => {
                template_id == artifact_id
                    || system_prompt_id == artifact_id
                    || skill_ids.iter().any(|id| id == artifact_id)
                    || document_ids.iter().any(|id| id == artifact_id)
            }
            EventPayload::AttemptContextOverridesApplied {
                template_document_ids,
                included_document_ids,
                excluded_document_ids,
                resolved_document_ids,
                ..
            } => {
                template_document_ids.iter().any(|id| id == artifact_id)
                    || included_document_ids.iter().any(|id| id == artifact_id)
                    || excluded_document_ids.iter().any(|id| id == artifact_id)
                    || resolved_document_ids.iter().any(|id| id == artifact_id)
            }
            _ => false,
        }
    }

    fn matches_template_id(payload: &EventPayload, template_id: &str) -> bool {
        match payload {
            EventPayload::TemplateInstantiated {
                template_id: id, ..
            } => id == template_id,
            EventPayload::GovernanceArtifactUpserted {
                artifact_kind,
                artifact_key,
                ..
            }
            | EventPayload::GovernanceArtifactDeleted {
                artifact_kind,
                artifact_key,
                ..
            } => artifact_kind == "template" && artifact_key == template_id,
            EventPayload::AttemptContextAssembled { manifest_json, .. } => {
                serde_json::from_str::<serde_json::Value>(manifest_json)
                    .ok()
                    .and_then(|manifest| {
                        manifest
                            .get("template_id")
                            .and_then(serde_json::Value::as_str)
                            .map(std::string::ToString::to_string)
                    })
                    .as_deref()
                    .is_some_and(|id| id == template_id)
            }
            _ => false,
        }
    }

    fn matches_rule_id(payload: &EventPayload, rule_id: &str) -> bool {
        matches!(
            payload,
            EventPayload::ConstitutionViolationDetected { rule_id: id, .. } if id == rule_id
        )
    }
}

/// Trait for event storage backends.
pub trait EventStore: Send + Sync {
    /// Appends an event to the store, returning its assigned ID.
    fn append(&self, event: Event) -> Result<EventId>;

    /// Reads events matching the filter.
    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>>;

    /// Streams events matching the filter.
    ///
    /// Implementations should emit matching historical events first, then continue
    /// yielding new events as they are appended.
    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>>;

    /// Reads all events in order.
    fn read_all(&self) -> Result<Vec<Event>>;
}

/// In-memory event store for testing.
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: Arc<RwLock<Vec<Event>>>,
}

impl InMemoryEventStore {
    /// Creates a new empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for InMemoryEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        let mut events = self.events.write().expect("lock poisoned");
        let next_seq = events.len() as u64;
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);
        if let Some(last) = events.last() {
            if event.metadata.timestamp <= last.metadata.timestamp {
                event.metadata.timestamp = last.metadata.timestamp + ChronoDuration::nanoseconds(1);
            }
        }
        let id = event.id();
        events.push(event);
        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let events = self.events.read().expect("lock poisoned");
        let mut result: Vec<Event> = events
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();
        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let events = self.events.read().expect("lock poisoned");
        Ok(events.clone())
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let events = Arc::clone(&self.events);

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen = 0usize;

            loop {
                let snapshot = {
                    let guard = events.read().expect("lock poisoned");
                    guard.clone()
                };

                for ev in snapshot.iter().skip(seen) {
                    if !filter.matches(ev) {
                        continue;
                    }

                    if tx.send(ev.clone()).is_err() {
                        return;
                    }

                    sent += 1;
                    if let Some(limit) = filter.limit {
                        if sent >= limit {
                            return;
                        }
                    }
                }

                seen = snapshot.len();
                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}

/// SQLite-backed event store (canonical) with optional JSONL mirror for inspectability.
#[derive(Debug)]
pub struct SqliteEventStore {
    db: PathBuf,
    legacy_mirror: PathBuf,
    write_lock: PathBuf,
}

impl SqliteEventStore {
    /// Creates or opens a SQLite-backed event store rooted at `base_dir`.
    ///
    /// This creates:
    /// - `base_dir/db.sqlite` (canonical event store)
    /// - `base_dir/events.jsonl` (append-only mirror for operators/tools)
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

    fn run_sql_batch_on_path(db_path: &Path, sql: &str) -> Result<()> {
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

/// File-based event store (append-only JSON lines).
#[derive(Debug)]
pub struct FileEventStore {
    path: PathBuf,
    cache: RwLock<Vec<Event>>,
}

impl FileEventStore {
    /// Creates or opens a file-based event store.
    ///
    /// # Errors
    /// Returns an error if the file cannot be created or read.
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.lock_shared()?;

        let mut content = String::new();
        {
            use std::io::Read;
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        file.unlock()?;

        let cache = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|line| {
                let normalized = normalize_concatenated_json_objects(line);
                serde_json::Deserializer::from_str(&normalized)
                    .into_iter::<Event>()
                    .collect::<Vec<_>>()
            })
            .collect::<std::result::Result<Vec<Event>, _>>()?;

        Ok(Self {
            path,
            cache: RwLock::new(cache),
        })
    }

    /// Returns the path to the event file.
    #[must_use]
    pub const fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for FileEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.lock_exclusive()?;

        let mut content = String::new();
        {
            use std::io::{Read, Seek};
            let _ = file.rewind();
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        let disk_events = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|line| {
                let normalized = normalize_concatenated_json_objects(line);
                serde_json::Deserializer::from_str(&normalized)
                    .into_iter::<Event>()
                    .collect::<Vec<_>>()
            })
            .collect::<std::result::Result<Vec<Event>, _>>()?;

        let mut cache = self.cache.write().expect("lock poisoned");
        cache.clone_from(&disk_events);

        let next_seq = cache.len() as u64;
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);
        if let Some(last) = cache.last() {
            if event.metadata.timestamp <= last.metadata.timestamp {
                event.metadata.timestamp = last.metadata.timestamp + ChronoDuration::nanoseconds(1);
            }
        }
        let id = event.id();

        let json = serde_json::to_string(&event)?;
        writeln!(file, "{json}")?;
        let _ = file.flush();
        let _ = file.unlock();

        cache.push(event);
        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        let cache = self.cache.read().expect("lock poisoned");
        let mut result: Vec<Event> = cache
            .iter()
            .filter(|e| filter.matches(e))
            .cloned()
            .collect();
        if let Some(limit) = filter.limit {
            result.truncate(limit);
        }
        Ok(result)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        let cache = self.cache.read().expect("lock poisoned");
        Ok(cache.clone())
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        let (tx, rx) = mpsc::channel();
        let filter = filter.clone();
        let path = self.path.clone();

        thread::spawn(move || {
            let mut sent = 0usize;
            let mut seen = 0usize;

            loop {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                let Ok(events) = content
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .flat_map(|line| {
                        let normalized = normalize_concatenated_json_objects(line);
                        serde_json::Deserializer::from_str(&normalized)
                            .into_iter::<Event>()
                            .collect::<Vec<_>>()
                    })
                    .collect::<std::result::Result<Vec<Event>, _>>()
                else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };

                for ev in events.iter().skip(seen) {
                    if !filter.matches(ev) {
                        continue;
                    }

                    if tx.send(ev.clone()).is_err() {
                        return;
                    }

                    sent += 1;
                    if let Some(limit) = filter.limit {
                        if sent >= limit {
                            return;
                        }
                    }
                }

                seen = events.len();
                thread::sleep(Duration::from_millis(200));
            }
        });

        Ok(rx)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct RegistryIndexDisk {
    projects: HashMap<String, String>,
}

/// Event store that maintains per-project and per-flow append-only logs plus an index.
///
/// Reads and streams are currently served from the global `events.jsonl` file to preserve
/// backwards compatibility and keep read ordering identical to the legacy store.
#[derive(Debug)]
pub struct IndexedEventStore {
    index_path: PathBuf,
    projects_dir: PathBuf,
    flows_dir: PathBuf,
    global: FileEventStore,
}

impl IndexedEventStore {
    /// Opens (or creates) an indexed store rooted at `base_dir`.
    ///
    /// This will create:
    /// - `base_dir/index.json`
    /// - `base_dir/projects/<project_id>/events.jsonl`
    /// - `base_dir/flows/<flow_id>/events.jsonl`
    /// - `base_dir/events.jsonl` (global, legacy-compatible)
    pub fn open(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;

        let index_path = base_dir.join("index.json");
        let projects_dir = base_dir.join("projects");
        let flows_dir = base_dir.join("flows");
        std::fs::create_dir_all(&projects_dir)?;
        std::fs::create_dir_all(&flows_dir)?;

        if !index_path.exists() {
            let disk = RegistryIndexDisk::default();
            std::fs::write(&index_path, serde_json::to_string_pretty(&disk)?)?;
        }

        let global = FileEventStore::open(base_dir.join("events.jsonl"))?;

        Ok(Self {
            index_path,
            projects_dir,
            flows_dir,
            global,
        })
    }

    fn project_log_rel(project_id: Uuid) -> String {
        format!("projects/{project_id}/events.jsonl")
    }

    fn flow_log_path(&self, flow_id: Uuid) -> PathBuf {
        self.flows_dir
            .join(flow_id.to_string())
            .join("events.jsonl")
    }

    fn ensure_project_index(&self, project_id: Uuid) -> Result<PathBuf> {
        use std::io::{Read, Seek, Write};

        let rel = Self::project_log_rel(project_id);
        let abs = self
            .projects_dir
            .join(project_id.to_string())
            .join("events.jsonl");

        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.index_path)?;
        file.lock_exclusive()?;

        let mut content = String::new();
        {
            let _ = file.rewind();
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        let mut disk: RegistryIndexDisk = if content.trim().is_empty() {
            RegistryIndexDisk::default()
        } else {
            serde_json::from_str(&content).unwrap_or_default()
        };

        disk.projects
            .entry(project_id.to_string())
            .or_insert_with(|| rel.clone());

        let json = serde_json::to_string_pretty(&disk)?;
        {
            let _ = file.rewind();
            file.set_len(0)?;
            file.write_all(json.as_bytes())?;
            let _ = file.flush();
        }
        let _ = file.unlock();

        Ok(abs)
    }

    fn append_mirror(path: &PathBuf, event: &Event) -> Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(path)?;
        file.lock_exclusive()?;
        let json = serde_json::to_string(event)?;
        writeln!(file, "{json}")?;
        let _ = file.flush();
        let _ = file.unlock();
        Ok(())
    }
}

#[allow(clippy::significant_drop_tightening)]
impl EventStore for IndexedEventStore {
    fn append(&self, mut event: Event) -> Result<EventId> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // Append to the global log using the same semantics as FileEventStore,
        // while keeping the fully-populated event value for mirroring.
        let mut file = OpenOptions::new()
            .read(true)
            .create(true)
            .append(true)
            .open(&self.global.path)?;
        file.lock_exclusive()?;

        let mut content = String::new();
        {
            use std::io::{Read, Seek};
            let _ = file.rewind();
            let mut reader = std::io::BufReader::new(&file);
            reader.read_to_string(&mut content)?;
        }

        let disk_events = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .flat_map(|line| {
                let normalized = normalize_concatenated_json_objects(line);
                serde_json::Deserializer::from_str(&normalized)
                    .into_iter::<Event>()
                    .collect::<Vec<_>>()
            })
            .collect::<std::result::Result<Vec<Event>, _>>()?;

        let mut cache = self.global.cache.write().expect("lock poisoned");
        cache.clone_from(&disk_events);

        let next_seq = cache.len() as u64;
        event.metadata.sequence = Some(next_seq);
        event.metadata.id = EventId::from_ordered_u64(next_seq);
        if let Some(last) = cache.last() {
            if event.metadata.timestamp <= last.metadata.timestamp {
                event.metadata.timestamp = last.metadata.timestamp + ChronoDuration::nanoseconds(1);
            }
        }
        let id = event.id();

        let json = serde_json::to_string(&event)?;
        writeln!(file, "{json}")?;
        let _ = file.flush();
        let _ = file.unlock();

        cache.push(event.clone());
        drop(cache);

        if let Some(project_id) = event.metadata.correlation.project_id {
            let project_path = self.ensure_project_index(project_id)?;
            Self::append_mirror(&project_path, &event)?;
        }
        if let Some(flow_id) = event.metadata.correlation.flow_id {
            let flow_path = self.flow_log_path(flow_id);
            Self::append_mirror(&flow_path, &event)?;
        }

        Ok(id)
    }

    fn read(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.global.read(filter)
    }

    fn stream(&self, filter: &EventFilter) -> Result<Receiver<Event>> {
        self.global.stream(filter)
    }

    fn read_all(&self) -> Result<Vec<Event>> {
        self.global.read_all()
    }
}

/// Thread-safe wrapper for any event store.
pub type SharedEventStore = Arc<dyn EventStore>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::{CorrelationIds, EventPayload};

    #[test]
    fn in_memory_store_append_and_read() {
        let store = InMemoryEventStore::new();
        let project_id = Uuid::new_v4();

        let event = Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        );

        let id = store.append(event).unwrap();
        let events = store.read_all().unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id(), id);
    }

    #[test]
    fn in_memory_store_filter_by_project() {
        let store = InMemoryEventStore::new();
        let project1 = Uuid::new_v4();
        let project2 = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project1,
                    name: "p1".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project1),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project2,
                    name: "p2".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project2),
            ))
            .unwrap();

        let filter = EventFilter::for_project(project1);
        let events = store.read(&filter).unwrap();

        assert_eq!(events.len(), 1);
    }

    #[test]
    fn in_memory_store_filter_by_graph() {
        let store = InMemoryEventStore::new();
        let project = Uuid::new_v4();
        let graph1 = Uuid::new_v4();
        let graph2 = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::TaskGraphCreated {
                    graph_id: graph1,
                    project_id: project,
                    name: "g1".to_string(),
                    description: None,
                },
                CorrelationIds::for_graph(project, graph1),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::TaskGraphCreated {
                    graph_id: graph2,
                    project_id: project,
                    name: "g2".to_string(),
                    description: None,
                },
                CorrelationIds::for_graph(project, graph2),
            ))
            .unwrap();

        let filter = EventFilter::for_graph(graph1);
        let events = store.read(&filter).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata.correlation.graph_id, Some(graph1));
    }

    #[test]
    fn in_memory_store_filter_by_time_range() {
        let store = InMemoryEventStore::new();
        let project = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project,
                    name: "p1".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project),
            ))
            .unwrap();
        store
            .append(Event::new(
                EventPayload::ProjectUpdated {
                    id: project,
                    name: Some("p2".to_string()),
                    description: None,
                },
                CorrelationIds::for_project(project),
            ))
            .unwrap();

        let all = store.read_all().unwrap();
        assert_eq!(all.len(), 2);
        let first_ts = all[0].metadata.timestamp;
        let second_ts = all[1].metadata.timestamp;

        let mut filter = EventFilter::all();
        filter.since = Some(second_ts);
        let since_events = store.read(&filter).unwrap();
        assert_eq!(since_events.len(), 1);
        assert_eq!(since_events[0].metadata.timestamp, second_ts);

        let mut filter = EventFilter::all();
        filter.until = Some(first_ts);
        let until_events = store.read(&filter).unwrap();
        assert_eq!(until_events.len(), 1);
        assert_eq!(until_events[0].metadata.timestamp, first_ts);
    }

    #[test]
    fn in_memory_store_filters_governance_artifact_template_and_rule_ids() {
        let store = InMemoryEventStore::new();
        let project = Uuid::new_v4();
        let flow = Uuid::new_v4();
        let task = Uuid::new_v4();
        let attempt = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::GovernanceArtifactUpserted {
                    project_id: Some(project),
                    scope: "project".to_string(),
                    artifact_kind: "document".to_string(),
                    artifact_key: "doc-1".to_string(),
                    path: "/tmp/doc-1.json".to_string(),
                    revision: 1,
                    schema_version: "governance.v1".to_string(),
                    projection_version: 1,
                },
                CorrelationIds::for_project(project),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::TemplateInstantiated {
                    project_id: project,
                    template_id: "tpl-1".to_string(),
                    system_prompt_id: "prompt-1".to_string(),
                    skill_ids: vec!["skill-1".to_string()],
                    document_ids: vec!["doc-1".to_string()],
                    schema_version: "governance.v1".to_string(),
                    projection_version: 1,
                },
                CorrelationIds::for_project(project),
            ))
            .unwrap();

        store
            .append(Event::new(
                EventPayload::ConstitutionViolationDetected {
                    project_id: project,
                    flow_id: Some(flow),
                    task_id: Some(task),
                    attempt_id: Some(attempt),
                    gate: "manual_check".to_string(),
                    rule_id: "rule-hard".to_string(),
                    rule_type: "forbidden_dependency".to_string(),
                    severity: "hard".to_string(),
                    message: "blocked".to_string(),
                    evidence: vec!["a -> b".to_string()],
                    remediation_hint: Some("fix dependency".to_string()),
                    blocked: true,
                },
                CorrelationIds::for_flow_task(project, flow, task),
            ))
            .unwrap();

        let mut artifact_filter = EventFilter::all();
        artifact_filter.artifact_id = Some("doc-1".to_string());
        let artifact_events = store.read(&artifact_filter).unwrap();
        assert_eq!(artifact_events.len(), 2);

        let mut template_filter = EventFilter::all();
        template_filter.template_id = Some("tpl-1".to_string());
        let template_events = store.read(&template_filter).unwrap();
        assert_eq!(template_events.len(), 1);
        assert!(matches!(
            template_events[0].payload,
            EventPayload::TemplateInstantiated { .. }
        ));

        let mut rule_filter = EventFilter::all();
        rule_filter.rule_id = Some("rule-hard".to_string());
        let rule_events = store.read(&rule_filter).unwrap();
        assert_eq!(rule_events.len(), 1);
        assert!(matches!(
            rule_events[0].payload,
            EventPayload::ConstitutionViolationDetected { .. }
        ));
    }

    #[test]
    fn file_store_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let project_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "persist-test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        );

        // Write
        {
            let store = FileEventStore::open(path.clone()).unwrap();
            store.append(event.clone()).unwrap();
        }

        // Reload
        {
            let store = FileEventStore::open(path).unwrap();
            let events = store.read_all().unwrap();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].payload, event.payload);
        }
    }

    #[test]
    fn file_store_ignores_unknown_event_payload_types() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");

        let project_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "persist-test".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        );

        let mut value = serde_json::to_value(&event).unwrap();
        value["payload"]["type"] = serde_json::json!("future_event_type");
        value["payload"]["some_new_field"] = serde_json::json!("some_value");
        let unknown_line = serde_json::to_string(&value).unwrap();

        std::fs::write(&path, format!("{unknown_line}\n")).unwrap();

        let store = FileEventStore::open(path).unwrap();
        let events = store.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, EventPayload::Unknown);
    }

    #[test]
    fn sqlite_store_append_read_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteEventStore::open(dir.path()).unwrap();
        let project_id = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "db-project".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ))
            .unwrap();

        let events = store.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].metadata.sequence, Some(0));

        let store_reloaded = SqliteEventStore::open(dir.path()).unwrap();
        let events_reloaded = store_reloaded.read_all().unwrap();
        assert_eq!(events_reloaded.len(), 1);
        assert!(matches!(
            events_reloaded[0].payload,
            EventPayload::ProjectCreated { .. }
        ));
    }

    #[test]
    fn sqlite_store_mirrors_legacy_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteEventStore::open(dir.path()).unwrap();
        let project_id = Uuid::new_v4();

        store
            .append(Event::new(
                EventPayload::ProjectCreated {
                    id: project_id,
                    name: "mirror-project".to_string(),
                    description: None,
                },
                CorrelationIds::for_project(project_id),
            ))
            .unwrap();

        let mirror_path = dir.path().join("events.jsonl");
        let mirror = std::fs::read_to_string(mirror_path).unwrap();
        assert!(mirror.contains("\"type\":\"project_created\""));
    }
}
