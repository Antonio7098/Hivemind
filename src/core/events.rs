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
use crate::core::workflow::{
    WorkflowContextSnapshot, WorkflowContextState, WorkflowDataValue, WorkflowDefinition,
    WorkflowOutputBagEntry, WorkflowRun, WorkflowSignal, WorkflowStepContextSnapshot,
    WorkflowStepState, WorkflowWaitStatus,
};
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
    use crate::core::workflow::{
        WorkflowContextSnapshot, WorkflowContextState, WorkflowDataValue, WorkflowDefinition,
        WorkflowOutputBagEntry, WorkflowRun, WorkflowSignal, WorkflowStepContextSnapshot,
        WorkflowStepDefinition, WorkflowStepKind, WorkflowStepState, WorkflowWaitCondition,
        WorkflowWaitStatus,
    };

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
    fn correlation_ids_for_workflow_run_include_workflow_lineage() {
        let project_id = Uuid::new_v4();
        let workflow_id = Uuid::new_v4();
        let workflow_run_id = Uuid::new_v4();
        let corr = CorrelationIds::for_workflow_run(project_id, workflow_id, workflow_run_id);
        assert_eq!(corr.project_id, Some(project_id));
        assert_eq!(corr.workflow_id, Some(workflow_id));
        assert_eq!(corr.workflow_run_id, Some(workflow_run_id));
        assert_eq!(corr.root_workflow_run_id, Some(workflow_run_id));
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

    #[test]
    fn workflow_payloads_roundtrip_through_serde() {
        let project_id = Uuid::new_v4();
        let mut definition = WorkflowDefinition::new(project_id, "demo-workflow", None);
        let step = WorkflowStepDefinition::new("root", WorkflowStepKind::Task);
        let step_id = step.id;
        definition.add_step(step);
        let run = WorkflowRun::new_root(&definition);
        let step_run_id = run.step_runs.get(&step_id).unwrap().id;
        let context = WorkflowContextState::initialize(
            "test_context".to_string(),
            1,
            BTreeMap::from([(
                "goal".to_string(),
                WorkflowDataValue::new(
                    "text/plain",
                    1,
                    serde_json::Value::String("demo".to_string()),
                )
                .unwrap(),
            )]),
            Utc::now(),
            "test_init",
        );
        let snapshot = WorkflowContextSnapshot::new(
            2,
            context.current_snapshot.values.clone(),
            "test_snapshot",
            Some(step_id),
            Some(step_run_id),
            Utc::now(),
        );
        let step_snapshot = WorkflowStepContextSnapshot::new(
            run.id,
            step_id,
            step_run_id,
            context.current_snapshot.snapshot_hash.clone(),
            run.output_bag.bag_hash.clone(),
            context.current_snapshot.values.clone(),
            Vec::new(),
            Utc::now(),
        );
        let entry = WorkflowOutputBagEntry {
            entry_id: Uuid::new_v4(),
            workflow_run_id: run.id,
            producer_step_id: step_id,
            producer_step_run_id: step_run_id,
            branch_step_id: Some(step_id),
            join_step_id: None,
            output_name: "result".to_string(),
            tags: vec!["test".to_string()],
            payload: WorkflowDataValue::new(
                "text/plain",
                1,
                serde_json::Value::String("ok".to_string()),
            )
            .unwrap(),
            event_sequence: 1,
            appended_at: Utc::now(),
        };

        let payloads = vec![
            EventPayload::WorkflowDefinitionCreated {
                definition: definition.clone(),
            },
            EventPayload::WorkflowConditionEvaluated {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                inputs: BTreeMap::new(),
                result: true,
                chosen_path: Some("yes".to_string()),
            },
            EventPayload::WorkflowWaitActivated {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                wait_status: WorkflowWaitCondition::Signal {
                    signal_name: "resume".to_string(),
                    payload_schema: Some("text/plain".to_string()),
                    payload_schema_version: Some(1),
                }
                .to_wait_status(Utc::now()),
            },
            EventPayload::WorkflowWaitCompleted {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                wait_status: WorkflowWaitStatus {
                    condition: WorkflowWaitCondition::Timer { duration_secs: 1 },
                    activated_at: Utc::now(),
                    resume_at: Some(Utc::now()),
                    completed_at: Some(Utc::now()),
                    completion_reason: Some("timer_elapsed".to_string()),
                    signal: None,
                },
            },
            EventPayload::WorkflowSignalReceived {
                workflow_run_id: run.id,
                signal: WorkflowSignal {
                    signal_name: "resume".to_string(),
                    idempotency_key: "key-1".to_string(),
                    payload: None,
                    step_id: Some(step_id),
                    emitted_at: Utc::now(),
                    emitted_by: "test".to_string(),
                },
            },
            EventPayload::WorkflowDefinitionUpdated {
                definition: definition.clone(),
            },
            EventPayload::WorkflowRunCreated { run: run.clone() },
            EventPayload::WorkflowRunStarted {
                workflow_run_id: run.id,
            },
            EventPayload::WorkflowRunPaused {
                workflow_run_id: run.id,
            },
            EventPayload::WorkflowRunResumed {
                workflow_run_id: run.id,
            },
            EventPayload::WorkflowRunCompleted {
                workflow_run_id: run.id,
            },
            EventPayload::WorkflowRunAborted {
                workflow_run_id: run.id,
                reason: Some("manual".to_string()),
                forced: true,
            },
            EventPayload::WorkflowContextInitialized {
                workflow_run_id: run.id,
                context,
            },
            EventPayload::WorkflowContextSnapshotCaptured {
                workflow_run_id: run.id,
                snapshot,
            },
            EventPayload::WorkflowStepInputsResolved {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                snapshot: step_snapshot,
            },
            EventPayload::WorkflowOutputAppended {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                entry,
            },
            EventPayload::WorkflowStepStateChanged {
                workflow_run_id: run.id,
                step_id,
                step_run_id,
                state: WorkflowStepState::Succeeded,
                reason: Some("done".to_string()),
            },
        ];

        for payload in payloads {
            let json = serde_json::to_string(&payload).expect("serialize workflow payload");
            let restored: EventPayload =
                serde_json::from_str(&json).expect("deserialize workflow payload");
            assert_eq!(payload, restored);
        }
    }
}
