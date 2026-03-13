use super::*;

#[allow(clippy::too_many_lines)]
pub(super) fn payload_category(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ChatSessionCreated { .. }
        | EventPayload::ChatSessionTitleUpdated { .. }
        | EventPayload::ChatMessageAppended { .. }
        | EventPayload::ChatStreamChunkAppended { .. } => "chat",

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
        | EventPayload::WorktreeCleanupPerformed { .. }
        | EventPayload::WorktreeTurnRefRestored { .. } => "flow",

        EventPayload::WorkflowDefinitionCreated { .. }
        | EventPayload::WorkflowDefinitionUpdated { .. }
        | EventPayload::WorkflowConditionEvaluated { .. }
        | EventPayload::WorkflowWaitActivated { .. }
        | EventPayload::WorkflowWaitCompleted { .. }
        | EventPayload::WorkflowSignalReceived { .. }
        | EventPayload::WorkflowRunCreated { .. }
        | EventPayload::WorkflowRunStarted { .. }
        | EventPayload::WorkflowRunPaused { .. }
        | EventPayload::WorkflowRunResumed { .. }
        | EventPayload::WorkflowRunCompleted { .. }
        | EventPayload::WorkflowRunAborted { .. }
        | EventPayload::WorkflowContextInitialized { .. }
        | EventPayload::WorkflowContextSnapshotCaptured { .. }
        | EventPayload::WorkflowStepInputsResolved { .. }
        | EventPayload::WorkflowOutputAppended { .. }
        | EventPayload::WorkflowStepStateChanged { .. } => "workflow",

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
        | EventPayload::NativeBudgetThresholdReached { .. }
        | EventPayload::NativeHistoryCompactionRecorded { .. }
        | EventPayload::NativeTurnSummaryRecorded { .. }
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
        | EventPayload::RuntimeCommandCompleted { .. }
        | EventPayload::RuntimeSessionObserved { .. }
        | EventPayload::RuntimeTurnCompleted { .. }
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
