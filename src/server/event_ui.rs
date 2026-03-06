use super::*;

fn payload_pascal_type(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ErrorOccurred { .. } => "ErrorOccurred",
        EventPayload::ProjectCreated { .. } => "ProjectCreated",
        EventPayload::ProjectUpdated { .. } => "ProjectUpdated",
        EventPayload::ProjectDeleted { .. } => "ProjectDeleted",
        EventPayload::ProjectRuntimeConfigured { .. } => "ProjectRuntimeConfigured",
        EventPayload::ProjectRuntimeRoleConfigured { .. } => "ProjectRuntimeRoleConfigured",
        EventPayload::GlobalRuntimeConfigured { .. } => "GlobalRuntimeConfigured",
        EventPayload::RepositoryAttached { .. } => "RepositoryAttached",
        EventPayload::RepositoryDetached { .. } => "RepositoryDetached",
        EventPayload::GovernanceProjectStorageInitialized { .. } => {
            "GovernanceProjectStorageInitialized"
        }
        EventPayload::GovernanceArtifactUpserted { .. } => "GovernanceArtifactUpserted",
        EventPayload::GovernanceArtifactDeleted { .. } => "GovernanceArtifactDeleted",
        EventPayload::GovernanceAttachmentLifecycleUpdated { .. } => {
            "GovernanceAttachmentLifecycleUpdated"
        }
        EventPayload::GovernanceStorageMigrated { .. } => "GovernanceStorageMigrated",
        EventPayload::GovernanceSnapshotCreated { .. } => "GovernanceSnapshotCreated",
        EventPayload::GovernanceSnapshotRestored { .. } => "GovernanceSnapshotRestored",
        EventPayload::GovernanceDriftDetected { .. } => "GovernanceDriftDetected",
        EventPayload::GovernanceRepairApplied { .. } => "GovernanceRepairApplied",
        EventPayload::GraphSnapshotStarted { .. } => "GraphSnapshotStarted",
        EventPayload::GraphSnapshotCompleted { .. } => "GraphSnapshotCompleted",
        EventPayload::GraphSnapshotFailed { .. } => "GraphSnapshotFailed",
        EventPayload::GraphSnapshotDiffDetected { .. } => "GraphSnapshotDiffDetected",
        EventPayload::GraphQueryExecuted { .. } => "GraphQueryExecuted",
        EventPayload::ConstitutionInitialized { .. } => "ConstitutionInitialized",
        EventPayload::ConstitutionUpdated { .. } => "ConstitutionUpdated",
        EventPayload::ConstitutionValidated { .. } => "ConstitutionValidated",
        EventPayload::ConstitutionViolationDetected { .. } => "ConstitutionViolationDetected",
        EventPayload::TemplateInstantiated { .. } => "TemplateInstantiated",
        EventPayload::AttemptContextOverridesApplied { .. } => "AttemptContextOverridesApplied",
        EventPayload::ContextWindowCreated { .. } => "ContextWindowCreated",
        EventPayload::ContextOpApplied { .. } => "ContextOpApplied",
        EventPayload::ContextWindowSnapshotCreated { .. } => "ContextWindowSnapshotCreated",
        EventPayload::AttemptContextAssembled { .. } => "AttemptContextAssembled",
        EventPayload::AttemptContextTruncated { .. } => "AttemptContextTruncated",
        EventPayload::AttemptContextDelivered { .. } => "AttemptContextDelivered",
        EventPayload::TaskCreated { .. } => "TaskCreated",
        EventPayload::TaskUpdated { .. } => "TaskUpdated",
        EventPayload::TaskRuntimeConfigured { .. } => "TaskRuntimeConfigured",
        EventPayload::TaskRuntimeRoleConfigured { .. } => "TaskRuntimeRoleConfigured",
        EventPayload::TaskRuntimeCleared { .. } => "TaskRuntimeCleared",
        EventPayload::TaskRuntimeRoleCleared { .. } => "TaskRuntimeRoleCleared",
        EventPayload::TaskRunModeSet { .. } => "TaskRunModeSet",
        EventPayload::TaskClosed { .. } => "TaskClosed",
        EventPayload::TaskDeleted { .. } => "TaskDeleted",
        EventPayload::TaskGraphCreated { .. } => "TaskGraphCreated",
        EventPayload::TaskAddedToGraph { .. } => "TaskAddedToGraph",
        EventPayload::DependencyAdded { .. } => "DependencyAdded",
        EventPayload::GraphTaskCheckAdded { .. } => "GraphTaskCheckAdded",
        EventPayload::ScopeAssigned { .. } => "ScopeAssigned",
        EventPayload::TaskGraphValidated { .. } => "TaskGraphValidated",
        EventPayload::TaskGraphLocked { .. } => "TaskGraphLocked",
        EventPayload::TaskGraphDeleted { .. } => "TaskGraphDeleted",
        EventPayload::TaskFlowCreated { .. } => "TaskFlowCreated",
        EventPayload::TaskFlowDependencyAdded { .. } => "TaskFlowDependencyAdded",
        EventPayload::TaskFlowRunModeSet { .. } => "TaskFlowRunModeSet",
        EventPayload::TaskFlowRuntimeConfigured { .. } => "TaskFlowRuntimeConfigured",
        EventPayload::TaskFlowRuntimeCleared { .. } => "TaskFlowRuntimeCleared",
        EventPayload::TaskFlowStarted { .. } => "TaskFlowStarted",
        EventPayload::TaskFlowPaused { .. } => "TaskFlowPaused",
        EventPayload::TaskFlowResumed { .. } => "TaskFlowResumed",
        EventPayload::TaskFlowCompleted { .. } => "TaskFlowCompleted",
        EventPayload::TaskFlowAborted { .. } => "TaskFlowAborted",
        EventPayload::TaskFlowDeleted { .. } => "TaskFlowDeleted",
        EventPayload::TaskReady { .. } => "TaskReady",
        EventPayload::TaskBlocked { .. } => "TaskBlocked",
        EventPayload::ScopeConflictDetected { .. } => "ScopeConflictDetected",
        EventPayload::TaskSchedulingDeferred { .. } => "TaskSchedulingDeferred",
        EventPayload::TaskExecutionStateChanged { .. } => "TaskExecutionStateChanged",
        EventPayload::TaskExecutionStarted { .. } => "TaskExecutionStarted",
        EventPayload::TaskExecutionSucceeded { .. } => "TaskExecutionSucceeded",
        EventPayload::TaskExecutionFailed { .. } => "TaskExecutionFailed",
        EventPayload::AttemptStarted { .. } => "AttemptStarted",
        EventPayload::BaselineCaptured { .. } => "BaselineCaptured",
        EventPayload::FileModified { .. } => "FileModified",
        EventPayload::DiffComputed { .. } => "DiffComputed",
        EventPayload::CheckStarted { .. } => "CheckStarted",
        EventPayload::CheckCompleted { .. } => "CheckCompleted",
        EventPayload::MergeCheckStarted { .. } => "MergeCheckStarted",
        EventPayload::MergeCheckCompleted { .. } => "MergeCheckCompleted",
        EventPayload::CheckpointDeclared { .. } => "CheckpointDeclared",
        EventPayload::CheckpointActivated { .. } => "CheckpointActivated",
        EventPayload::CheckpointCompleted { .. } => "CheckpointCompleted",
        EventPayload::AllCheckpointsCompleted { .. } => "AllCheckpointsCompleted",
        EventPayload::CheckpointCommitCreated { .. } => "CheckpointCommitCreated",
        EventPayload::ScopeValidated { .. } => "ScopeValidated",
        EventPayload::ScopeViolationDetected { .. } => "ScopeViolationDetected",
        EventPayload::RetryContextAssembled { .. } => "RetryContextAssembled",
        EventPayload::TaskRetryRequested { .. } => "TaskRetryRequested",
        EventPayload::TaskAborted { .. } => "TaskAborted",
        EventPayload::HumanOverride { .. } => "HumanOverride",
        EventPayload::MergePrepared { .. } => "MergePrepared",
        EventPayload::TaskExecutionFrozen { .. } => "TaskExecutionFrozen",
        EventPayload::TaskIntegratedIntoFlow { .. } => "TaskIntegratedIntoFlow",
        EventPayload::MergeConflictDetected { .. } => "MergeConflictDetected",
        EventPayload::FlowFrozenForMerge { .. } => "FlowFrozenForMerge",
        EventPayload::FlowIntegrationLockAcquired { .. } => "FlowIntegrationLockAcquired",
        EventPayload::MergeApproved { .. } => "MergeApproved",
        EventPayload::MergeCompleted { .. } => "MergeCompleted",
        EventPayload::WorktreeCleanupPerformed { .. } => "WorktreeCleanupPerformed",
        EventPayload::RuntimeCapabilitiesEvaluated { .. } => "RuntimeCapabilitiesEvaluated",
        EventPayload::AgentInvocationStarted { .. } => "AgentInvocationStarted",
        EventPayload::AgentTurnStarted { .. } => "AgentTurnStarted",
        EventPayload::ModelRequestPrepared { .. } => "ModelRequestPrepared",
        EventPayload::ModelResponseReceived { .. } => "ModelResponseReceived",
        EventPayload::ToolCallRequested { .. } => "ToolCallRequested",
        EventPayload::ToolCallStarted { .. } => "ToolCallStarted",
        EventPayload::ToolCallCompleted { .. } => "ToolCallCompleted",
        EventPayload::ToolCallFailed { .. } => "ToolCallFailed",
        EventPayload::AgentTurnCompleted { .. } => "AgentTurnCompleted",
        EventPayload::AgentInvocationCompleted { .. } => "AgentInvocationCompleted",
        EventPayload::RuntimeStarted { .. } => "RuntimeStarted",
        EventPayload::RuntimeEnvironmentPrepared { .. } => "RuntimeEnvironmentPrepared",
        EventPayload::RuntimeOutputChunk { .. } => "RuntimeOutputChunk",
        EventPayload::RuntimeInputProvided { .. } => "RuntimeInputProvided",
        EventPayload::RuntimeInterrupted { .. } => "RuntimeInterrupted",
        EventPayload::RuntimeExited { .. } => "RuntimeExited",
        EventPayload::RuntimeTerminated { .. } => "RuntimeTerminated",
        EventPayload::RuntimeErrorClassified { .. } => "RuntimeErrorClassified",
        EventPayload::RuntimeRecoveryScheduled { .. } => "RuntimeRecoveryScheduled",
        EventPayload::RuntimeFilesystemObserved { .. } => "RuntimeFilesystemObserved",
        EventPayload::RuntimeCommandObserved { .. } => "RuntimeCommandObserved",
        EventPayload::RuntimeToolCallObserved { .. } => "RuntimeToolCallObserved",
        EventPayload::RuntimeTodoSnapshotUpdated { .. } => "RuntimeTodoSnapshotUpdated",
        EventPayload::RuntimeNarrativeOutputObserved { .. } => "RuntimeNarrativeOutputObserved",
        EventPayload::Unknown => "Unknown",
    }
}

#[allow(clippy::too_many_lines)]
fn payload_category(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ErrorOccurred { .. } => "error",

        EventPayload::ProjectCreated { .. }
        | EventPayload::ProjectUpdated { .. }
        | EventPayload::ProjectDeleted { .. }
        | EventPayload::ProjectRuntimeConfigured { .. }
        | EventPayload::ProjectRuntimeRoleConfigured { .. }
        | EventPayload::GlobalRuntimeConfigured { .. }
        | EventPayload::RepositoryAttached { .. }
        | EventPayload::RepositoryDetached { .. }
        | EventPayload::GovernanceProjectStorageInitialized { .. }
        | EventPayload::GovernanceArtifactUpserted { .. }
        | EventPayload::GovernanceArtifactDeleted { .. }
        | EventPayload::GovernanceAttachmentLifecycleUpdated { .. }
        | EventPayload::GovernanceStorageMigrated { .. }
        | EventPayload::GovernanceSnapshotCreated { .. }
        | EventPayload::GovernanceSnapshotRestored { .. }
        | EventPayload::GovernanceDriftDetected { .. }
        | EventPayload::GovernanceRepairApplied { .. }
        | EventPayload::GraphSnapshotStarted { .. }
        | EventPayload::GraphSnapshotCompleted { .. }
        | EventPayload::GraphSnapshotFailed { .. }
        | EventPayload::GraphSnapshotDiffDetected { .. }
        | EventPayload::GraphQueryExecuted { .. }
        | EventPayload::ConstitutionInitialized { .. }
        | EventPayload::ConstitutionUpdated { .. }
        | EventPayload::ConstitutionValidated { .. }
        | EventPayload::TemplateInstantiated { .. } => "project",

        EventPayload::TaskCreated { .. }
        | EventPayload::TaskUpdated { .. }
        | EventPayload::TaskRuntimeConfigured { .. }
        | EventPayload::TaskRuntimeRoleConfigured { .. }
        | EventPayload::TaskRuntimeCleared { .. }
        | EventPayload::TaskRuntimeRoleCleared { .. }
        | EventPayload::TaskRunModeSet { .. }
        | EventPayload::TaskClosed { .. }
        | EventPayload::TaskDeleted { .. } => "task",

        EventPayload::TaskGraphCreated { .. }
        | EventPayload::TaskAddedToGraph { .. }
        | EventPayload::DependencyAdded { .. }
        | EventPayload::GraphTaskCheckAdded { .. }
        | EventPayload::ScopeAssigned { .. }
        | EventPayload::TaskGraphValidated { .. }
        | EventPayload::TaskGraphLocked { .. }
        | EventPayload::TaskGraphDeleted { .. } => "graph",

        EventPayload::TaskFlowCreated { .. }
        | EventPayload::TaskFlowDependencyAdded { .. }
        | EventPayload::TaskFlowRunModeSet { .. }
        | EventPayload::TaskFlowRuntimeConfigured { .. }
        | EventPayload::TaskFlowRuntimeCleared { .. }
        | EventPayload::TaskFlowStarted { .. }
        | EventPayload::TaskFlowPaused { .. }
        | EventPayload::TaskFlowResumed { .. }
        | EventPayload::TaskFlowCompleted { .. }
        | EventPayload::TaskFlowAborted { .. }
        | EventPayload::TaskFlowDeleted { .. }
        | EventPayload::WorktreeCleanupPerformed { .. } => "flow",

        EventPayload::TaskReady { .. }
        | EventPayload::TaskBlocked { .. }
        | EventPayload::ScopeConflictDetected { .. }
        | EventPayload::TaskSchedulingDeferred { .. }
        | EventPayload::TaskExecutionStateChanged { .. }
        | EventPayload::TaskExecutionStarted { .. }
        | EventPayload::TaskExecutionSucceeded { .. }
        | EventPayload::TaskExecutionFailed { .. }
        | EventPayload::AttemptStarted { .. }
        | EventPayload::AttemptContextOverridesApplied { .. }
        | EventPayload::ContextWindowCreated { .. }
        | EventPayload::ContextOpApplied { .. }
        | EventPayload::ContextWindowSnapshotCreated { .. }
        | EventPayload::AttemptContextAssembled { .. }
        | EventPayload::AttemptContextTruncated { .. }
        | EventPayload::AttemptContextDelivered { .. }
        | EventPayload::CheckpointDeclared { .. }
        | EventPayload::CheckpointActivated { .. }
        | EventPayload::CheckpointCompleted { .. }
        | EventPayload::AllCheckpointsCompleted { .. }
        | EventPayload::RetryContextAssembled { .. }
        | EventPayload::TaskRetryRequested { .. }
        | EventPayload::TaskAborted { .. } => "execution",

        EventPayload::CheckStarted { .. }
        | EventPayload::CheckCompleted { .. }
        | EventPayload::ConstitutionViolationDetected { .. }
        | EventPayload::HumanOverride { .. } => "verification",

        EventPayload::ScopeValidated { .. } | EventPayload::ScopeViolationDetected { .. } => {
            "scope"
        }

        EventPayload::MergePrepared { .. }
        | EventPayload::MergeCheckStarted { .. }
        | EventPayload::MergeCheckCompleted { .. }
        | EventPayload::TaskExecutionFrozen { .. }
        | EventPayload::TaskIntegratedIntoFlow { .. }
        | EventPayload::MergeConflictDetected { .. }
        | EventPayload::FlowFrozenForMerge { .. }
        | EventPayload::FlowIntegrationLockAcquired { .. }
        | EventPayload::MergeApproved { .. }
        | EventPayload::MergeCompleted { .. } => "merge",

        EventPayload::RuntimeCapabilitiesEvaluated { .. }
        | EventPayload::AgentInvocationStarted { .. }
        | EventPayload::AgentTurnStarted { .. }
        | EventPayload::ModelRequestPrepared { .. }
        | EventPayload::ModelResponseReceived { .. }
        | EventPayload::ToolCallRequested { .. }
        | EventPayload::ToolCallStarted { .. }
        | EventPayload::ToolCallCompleted { .. }
        | EventPayload::ToolCallFailed { .. }
        | EventPayload::AgentTurnCompleted { .. }
        | EventPayload::AgentInvocationCompleted { .. }
        | EventPayload::RuntimeStarted { .. }
        | EventPayload::RuntimeEnvironmentPrepared { .. }
        | EventPayload::RuntimeOutputChunk { .. }
        | EventPayload::RuntimeInputProvided { .. }
        | EventPayload::RuntimeInterrupted { .. }
        | EventPayload::RuntimeExited { .. }
        | EventPayload::RuntimeTerminated { .. }
        | EventPayload::RuntimeErrorClassified { .. }
        | EventPayload::RuntimeRecoveryScheduled { .. }
        | EventPayload::RuntimeFilesystemObserved { .. }
        | EventPayload::RuntimeCommandObserved { .. }
        | EventPayload::RuntimeToolCallObserved { .. }
        | EventPayload::RuntimeTodoSnapshotUpdated { .. }
        | EventPayload::RuntimeNarrativeOutputObserved { .. }
        | EventPayload::Unknown => "runtime",

        EventPayload::FileModified { .. }
        | EventPayload::DiffComputed { .. }
        | EventPayload::CheckpointCommitCreated { .. }
        | EventPayload::BaselineCaptured { .. } => "filesystem",
    }
}

fn payload_map(payload: &EventPayload) -> Result<HashMap<String, Value>> {
    let mut v = serde_json::to_value(payload).map_err(|e| {
        HivemindError::system(
            "payload_serialize_failed",
            e.to_string(),
            "server:payload_map",
        )
    })?;

    match &mut v {
        Value::Object(map) => {
            map.remove("type");
            Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => Ok(HashMap::new()),
    }
}

pub(super) fn ui_event(event: &Event) -> Result<UiEvent> {
    Ok(UiEvent {
        id: event.metadata.id.to_string(),
        r#type: payload_pascal_type(&event.payload).to_string(),
        category: payload_category(&event.payload).to_string(),
        timestamp: event.metadata.timestamp,
        sequence: event.metadata.sequence,
        correlation: event.metadata.correlation.clone(),
        payload: payload_map(&event.payload)?,
    })
}

pub(super) fn build_ui_state(registry: &Registry, events_limit: usize) -> Result<UiState> {
    let state = registry.state()?;

    let mut projects: Vec<Project> = state.projects.into_values().collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();

    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();

    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();

    let mut merge_states: Vec<MergeState> = state.merge_states.into_values().collect();
    merge_states.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merge_states.reverse();

    let events = registry.list_events(None, events_limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();

    Ok(UiState {
        projects,
        tasks,
        graphs,
        flows,
        merge_states,
        events: ui_events,
    })
}
