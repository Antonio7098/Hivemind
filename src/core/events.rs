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
use std::collections::HashMap;
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

/// Payload types for different events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    /// A failure occurred and was recorded.
    ErrorOccurred {
        error: HivemindError,
    },

    /// A new project was created.
    ProjectCreated {
        id: Uuid,
        name: String,
        description: Option<String>,
    },
    /// A project was updated.
    ProjectUpdated {
        id: Uuid,
        name: Option<String>,
        description: Option<String>,
    },
    ProjectRuntimeConfigured {
        project_id: Uuid,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    ProjectRuntimeRoleConfigured {
        project_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    GlobalRuntimeConfigured {
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    /// A new task was created.
    TaskCreated {
        id: Uuid,
        project_id: Uuid,
        title: String,
        description: Option<String>,
        #[serde(default)]
        scope: Option<Scope>,
    },
    /// A task was updated.
    TaskUpdated {
        id: Uuid,
        title: Option<String>,
        description: Option<String>,
    },
    TaskRuntimeConfigured {
        task_id: Uuid,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
    },
    TaskRuntimeRoleConfigured {
        task_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
    },
    TaskRuntimeCleared {
        task_id: Uuid,
    },
    TaskRuntimeRoleCleared {
        task_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
    },
    TaskRunModeSet {
        task_id: Uuid,
        mode: RunMode,
    },
    /// A task was closed.
    TaskClosed {
        id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },
    /// A repository was attached to a project.
    RepositoryAttached {
        project_id: Uuid,
        path: String,
        name: String,
        #[serde(default)]
        access_mode: RepoAccessMode,
    },
    /// A repository was detached from a project.
    RepositoryDetached {
        project_id: Uuid,
        name: String,
    },

    TaskGraphCreated {
        graph_id: Uuid,
        project_id: Uuid,
        name: String,
        #[serde(default)]
        description: Option<String>,
    },
    TaskAddedToGraph {
        graph_id: Uuid,
        task: GraphTask,
    },
    DependencyAdded {
        graph_id: Uuid,
        from_task: Uuid,
        to_task: Uuid,
    },
    GraphTaskCheckAdded {
        graph_id: Uuid,
        task_id: Uuid,
        check: CheckConfig,
    },
    ScopeAssigned {
        graph_id: Uuid,
        task_id: Uuid,
        scope: Scope,
    },

    TaskGraphValidated {
        graph_id: Uuid,
        project_id: Uuid,
        valid: bool,
        #[serde(default)]
        issues: Vec<String>,
    },

    TaskGraphLocked {
        graph_id: Uuid,
        project_id: Uuid,
    },

    TaskFlowCreated {
        flow_id: Uuid,
        graph_id: Uuid,
        project_id: Uuid,
        #[serde(default)]
        name: Option<String>,
        task_ids: Vec<Uuid>,
    },
    TaskFlowDependencyAdded {
        flow_id: Uuid,
        depends_on_flow_id: Uuid,
    },
    TaskFlowRunModeSet {
        flow_id: Uuid,
        mode: RunMode,
    },
    TaskFlowRuntimeConfigured {
        flow_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
        adapter_name: String,
        binary_path: String,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: HashMap<String, String>,
        timeout_ms: u64,
        #[serde(default = "default_max_parallel_tasks")]
        max_parallel_tasks: u16,
    },
    TaskFlowRuntimeCleared {
        flow_id: Uuid,
        #[serde(default = "default_runtime_role_worker")]
        role: RuntimeRole,
    },
    TaskFlowStarted {
        flow_id: Uuid,
        #[serde(default)]
        base_revision: Option<String>,
    },
    TaskFlowPaused {
        flow_id: Uuid,
        #[serde(default)]
        running_tasks: Vec<Uuid>,
    },
    TaskFlowResumed {
        flow_id: Uuid,
    },
    TaskFlowCompleted {
        flow_id: Uuid,
    },
    TaskFlowAborted {
        flow_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
        forced: bool,
    },

    TaskReady {
        flow_id: Uuid,
        task_id: Uuid,
    },
    TaskBlocked {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },
    ScopeConflictDetected {
        flow_id: Uuid,
        task_id: Uuid,
        conflicting_task_id: Uuid,
        severity: String,
        action: String,
        reason: String,
    },
    TaskSchedulingDeferred {
        flow_id: Uuid,
        task_id: Uuid,
        reason: String,
    },
    TaskExecutionStateChanged {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        from: TaskExecState,
        to: TaskExecState,
    },

    TaskExecutionStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
    },
    TaskExecutionSucceeded {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
    },
    TaskExecutionFailed {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        attempt_id: Option<Uuid>,
        #[serde(default)]
        reason: Option<String>,
    },

    AttemptStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
    },

    BaselineCaptured {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        baseline_id: Uuid,
        #[serde(default)]
        git_head: Option<String>,
        file_count: usize,
    },

    FileModified {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        path: String,
        change_type: ChangeType,
        #[serde(default)]
        old_hash: Option<String>,
        #[serde(default)]
        new_hash: Option<String>,
    },

    DiffComputed {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        diff_id: Uuid,
        baseline_id: Uuid,
        change_count: usize,
    },

    CheckStarted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        check_name: String,
        required: bool,
    },

    CheckCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        check_name: String,
        passed: bool,
        exit_code: i32,
        output: String,
        duration_ms: u64,
        required: bool,
    },

    MergeCheckStarted {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        check_name: String,
        required: bool,
    },

    MergeCheckCompleted {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        check_name: String,
        passed: bool,
        exit_code: i32,
        output: String,
        duration_ms: u64,
        required: bool,
    },

    TaskExecutionFrozen {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        commit_sha: Option<String>,
    },

    TaskIntegratedIntoFlow {
        flow_id: Uuid,
        task_id: Uuid,
        #[serde(default)]
        commit_sha: Option<String>,
    },

    MergeConflictDetected {
        flow_id: Uuid,
        #[serde(default)]
        task_id: Option<Uuid>,
        details: String,
    },

    FlowFrozenForMerge {
        flow_id: Uuid,
    },

    FlowIntegrationLockAcquired {
        flow_id: Uuid,
        operation: String,
    },

    CheckpointDeclared {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
        total: u32,
    },

    CheckpointActivated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
    },

    CheckpointCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        checkpoint_id: String,
        order: u32,
        commit_hash: String,
        timestamp: DateTime<Utc>,
        #[serde(default)]
        summary: Option<String>,
    },

    AllCheckpointsCompleted {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
    },

    CheckpointCommitCreated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        commit_sha: String,
    },

    ScopeValidated {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        verification_id: Uuid,
        verified_at: DateTime<Utc>,
        scope: Scope,
    },

    ScopeViolationDetected {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        verification_id: Uuid,
        verified_at: DateTime<Utc>,
        scope: Scope,
        #[serde(default)]
        violations: Vec<ScopeViolation>,
    },

    RetryContextAssembled {
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
        attempt_number: u32,
        max_attempts: u32,
        #[serde(default)]
        prior_attempt_ids: Vec<Uuid>,
        #[serde(default)]
        required_check_failures: Vec<String>,
        #[serde(default)]
        optional_check_failures: Vec<String>,
        #[serde(default)]
        runtime_exit_code: Option<i32>,
        #[serde(default)]
        runtime_terminated_reason: Option<String>,
        context: String,
    },

    TaskRetryRequested {
        task_id: Uuid,
        reset_count: bool,
        #[serde(default)]
        retry_mode: RetryMode,
    },
    TaskAborted {
        task_id: Uuid,
        #[serde(default)]
        reason: Option<String>,
    },

    HumanOverride {
        task_id: Uuid,
        override_type: String,
        decision: String,
        reason: String,
        #[serde(default)]
        user: Option<String>,
    },

    MergePrepared {
        flow_id: Uuid,
        #[serde(default)]
        target_branch: Option<String>,
        #[serde(default)]
        conflicts: Vec<String>,
    },
    MergeApproved {
        flow_id: Uuid,
        #[serde(default)]
        user: Option<String>,
    },
    MergeCompleted {
        flow_id: Uuid,
        #[serde(default)]
        commits: Vec<String>,
    },
    WorktreeCleanupPerformed {
        flow_id: Uuid,
        cleaned_worktrees: usize,
        forced: bool,
        dry_run: bool,
    },
    RuntimeStarted {
        adapter_name: String,
        task_id: Uuid,
        attempt_id: Uuid,
    },
    RuntimeOutputChunk {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        content: String,
    },
    RuntimeInputProvided {
        attempt_id: Uuid,
        content: String,
    },
    RuntimeInterrupted {
        attempt_id: Uuid,
    },
    RuntimeExited {
        attempt_id: Uuid,
        exit_code: i32,
        duration_ms: u64,
    },
    RuntimeTerminated {
        attempt_id: Uuid,
        reason: String,
    },
    RuntimeFilesystemObserved {
        attempt_id: Uuid,
        #[serde(default)]
        files_created: Vec<PathBuf>,
        #[serde(default)]
        files_modified: Vec<PathBuf>,
        #[serde(default)]
        files_deleted: Vec<PathBuf>,
    },
    RuntimeCommandObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        command: String,
    },
    RuntimeToolCallObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        tool_name: String,
        details: String,
    },
    RuntimeTodoSnapshotUpdated {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        #[serde(default)]
        items: Vec<String>,
    },
    RuntimeNarrativeOutputObserved {
        attempt_id: Uuid,
        stream: RuntimeOutputStream,
        content: String,
    },

    #[serde(other)]
    Unknown,
}

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
}
