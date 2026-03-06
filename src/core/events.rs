//! Event definitions and types.
//!
//! All state in Hivemind is derived from events. Events are immutable,
//! append-only, and form the single source of truth.

use crate::core::diff::ChangeType;
use crate::core::enforcement::ScopeViolation;
use crate::core::error::HivemindError;
use crate::core::flow::{RetryMode, RunMode, TaskExecState};
use crate::core::graph::GraphTask;
use crate::core::scope::{RepoAccessMode, Scope};
use crate::core::verification::CheckConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use uuid::Uuid;

const fn default_max_parallel_tasks() -> u16 {
    1
}

const fn default_runtime_role_worker() -> RuntimeRole {
    RuntimeRole::Worker
}

/// Runtime role for model/runtime defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeRole {
    Worker,
    Validator,
}

/// Source used to resolve an effective runtime configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSelectionSource {
    TaskOverride,
    FlowDefault,
    ProjectDefault,
    GlobalDefault,
}

impl RuntimeSelectionSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskOverride => "task_override",
            Self::FlowDefault => "flow_default",
            Self::ProjectDefault => "project_default",
            Self::GlobalDefault => "global_default",
        }
    }
}

/// Correlation identifiers embedded in native runtime event payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEventCorrelation {
    pub project_id: Uuid,
    pub graph_id: Uuid,
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_id: Uuid,
}

/// Payload capture mode for native runtime event payload fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeEventPayloadCaptureMode {
    MetadataOnly,
    FullPayload,
}

/// Hash-addressed payload blob metadata used by native runtime events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBlobRef {
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub blob_path: String,
    #[serde(default)]
    pub payload: Option<String>,
}

/// Unique identifier for an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(Uuid);

impl EventId {
    /// Creates a new unique event ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a unique event ID that is ordered by the provided sequence.
    ///
    /// This preserves UUID wire format while allowing stores to guarantee a
    /// monotonic ordering property for event IDs within a log.
    #[must_use]
    pub fn from_ordered_u64(sequence: u64) -> Self {
        let mut bytes = *Uuid::new_v4().as_bytes();
        bytes[..8].copy_from_slice(&sequence.to_be_bytes());
        Self(Uuid::from_bytes(bytes))
    }

    /// Returns the inner UUID.
    #[must_use]
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Correlation IDs for tracing event relationships.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationIds {
    /// Project this event belongs to.
    pub project_id: Option<Uuid>,
    #[serde(default)]
    pub graph_id: Option<Uuid>,
    /// Flow this event belongs to.
    pub flow_id: Option<Uuid>,
    /// Task this event belongs to.
    pub task_id: Option<Uuid>,
    /// Attempt this event belongs to.
    pub attempt_id: Option<Uuid>,
}

impl CorrelationIds {
    /// Creates empty correlation IDs.
    #[must_use]
    pub fn none() -> Self {
        Self {
            project_id: None,
            graph_id: None,
            flow_id: None,
            task_id: None,
            attempt_id: None,
        }
    }

    /// Creates correlation IDs with only a project ID.
    #[must_use]
    pub fn for_project(project_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            task_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph(project_id: Uuid, graph_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: None,
            task_id: None,
            attempt_id: None,
        }
    }

    /// Creates correlation IDs with project and task.
    #[must_use]
    pub fn for_task(project_id: Uuid, task_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: None,
            task_id: Some(task_id),
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_flow(project_id: Uuid, flow_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: Some(flow_id),
            task_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow(project_id: Uuid, graph_id: Uuid, flow_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            task_id: None,
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_flow_task(project_id: Uuid, flow_id: Uuid, task_id: Uuid) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: None,
            flow_id: Some(flow_id),
            task_id: Some(task_id),
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow_task(
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
        task_id: Uuid,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            task_id: Some(task_id),
            attempt_id: None,
        }
    }

    #[must_use]
    pub fn for_graph_flow_task_attempt(
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            task_id: Some(task_id),
            attempt_id: Some(attempt_id),
        }
    }
}

/// Event metadata common to all events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique event identifier.
    pub id: EventId,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Correlation IDs for tracing.
    pub correlation: CorrelationIds,
    /// Sequence number within the event stream (assigned by store).
    pub sequence: Option<u64>,
}

impl EventMetadata {
    /// Creates new metadata with current timestamp.
    #[must_use]
    pub fn new(correlation: CorrelationIds) -> Self {
        Self {
            id: EventId::new(),
            timestamp: Utc::now(),
            correlation,
            sequence: None,
        }
    }
}

mod payload;
pub use payload::EventPayload;

/// Output stream for runtime output events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeOutputStream {
    Stdout,
    Stderr,
}

/// A complete event with metadata and payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Event metadata.
    pub metadata: EventMetadata,
    /// Event payload.
    pub payload: EventPayload,
}

impl Event {
    /// Creates a new event with the given payload and correlation.
    #[must_use]
    pub fn new(payload: EventPayload, correlation: CorrelationIds) -> Self {
        Self {
            metadata: EventMetadata::new(correlation),
            payload,
        }
    }

    /// Returns the event ID.
    #[must_use]
    pub fn id(&self) -> EventId {
        self.metadata.id
    }

    /// Returns the event timestamp.
    #[must_use]
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.metadata.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::flow::RetryMode;

    #[test]
    fn event_id_is_unique() {
        let id1 = EventId::new();
        let id2 = EventId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn event_serialization_roundtrip() {
        let event = Event::new(
            EventPayload::ProjectCreated {
                id: Uuid::new_v4(),
                name: "test-project".to_string(),
                description: Some("A test project".to_string()),
            },
            CorrelationIds::none(),
        );

        let json = serde_json::to_string(&event).expect("serialize");
        let restored: Event = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(event.payload, restored.payload);
    }

    #[test]
    fn correlation_ids_for_project() {
        let project_id = Uuid::new_v4();
        let corr = CorrelationIds::for_project(project_id);
        assert_eq!(corr.project_id, Some(project_id));
        assert!(corr.task_id.is_none());
    }

    #[test]
    fn task_retry_payload_uses_task_retried_type_and_accepts_legacy_alias() {
        let payload = EventPayload::TaskRetryRequested {
            task_id: Uuid::new_v4(),
            reset_count: false,
            retry_mode: RetryMode::Continue,
        };

        let json = serde_json::to_value(&payload).expect("serialize retry payload");
        assert_eq!(
            json.get("type").and_then(serde_json::Value::as_str),
            Some("task_retried")
        );

        let task_id = Uuid::new_v4();
        let legacy_json = serde_json::json!({
            "type": "task_retry_requested",
            "task_id": task_id,
            "reset_count": true,
            "retry_mode": "clean"
        });
        let restored: EventPayload =
            serde_json::from_value(legacy_json).expect("deserialize legacy alias");
        assert!(matches!(
            restored,
            EventPayload::TaskRetryRequested {
                task_id: id,
                reset_count: true,
                retry_mode: RetryMode::Clean
            } if id == task_id
        ));
    }
}
