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

mod payload;
pub use payload::*;

mod runtime;
pub use runtime::*;
pub(crate) use runtime::{default_max_parallel_tasks, default_runtime_role_worker};
mod ids;
pub use ids::*;
mod model;
pub use model::*;

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
