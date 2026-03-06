//! `EventStore` trait and implementations.
//!
//! Event stores are the persistence layer for events. All state is derived
//! from events, so the event store is the single source of truth.

use crate::core::events::{Event, EventId, EventPayload};
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

mod sqlite_store;
pub use sqlite_store::*;

mod jsonl_stores;
pub use jsonl_stores::*;

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
    /// Filter `error_occurred` events by error category.
    pub error_type: Option<String>,
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
        if let Some(error_type) = self.error_type.as_deref() {
            if !Self::matches_error_type(&event.payload, error_type) {
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

    fn matches_error_type(payload: &EventPayload, error_type: &str) -> bool {
        matches!(
            payload,
            EventPayload::ErrorOccurred { error } if error.category.to_string() == error_type
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

/// Thread-safe wrapper for any event store.
pub type SharedEventStore = Arc<dyn EventStore>;

#[cfg(test)]
mod tests;
