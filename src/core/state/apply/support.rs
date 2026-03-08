use super::*;

impl AppState {
    pub(super) fn is_ignored_event(payload: &EventPayload) -> bool {
        matches!(
            payload,
            EventPayload::CheckStarted { .. }
                | EventPayload::GovernanceSnapshotCreated { .. }
                | EventPayload::GovernanceSnapshotRestored { .. }
                | EventPayload::GovernanceDriftDetected { .. }
                | EventPayload::GovernanceRepairApplied { .. }
                | EventPayload::GraphSnapshotStarted { .. }
                | EventPayload::GraphSnapshotCompleted { .. }
                | EventPayload::GraphSnapshotFailed { .. }
                | EventPayload::GraphSnapshotDiffDetected { .. }
                | EventPayload::GraphQueryExecuted { .. }
                | EventPayload::ConstitutionValidated { .. }
                | EventPayload::ConstitutionViolationDetected { .. }
                | EventPayload::TemplateInstantiated { .. }
                | EventPayload::AttemptContextOverridesApplied { .. }
                | EventPayload::ContextWindowCreated { .. }
                | EventPayload::ContextOpApplied { .. }
                | EventPayload::ContextWindowSnapshotCreated { .. }
                | EventPayload::AttemptContextAssembled { .. }
                | EventPayload::AttemptContextTruncated { .. }
                | EventPayload::AttemptContextDelivered { .. }
                | EventPayload::ErrorOccurred { .. }
                | EventPayload::TaskExecutionStarted { .. }
                | EventPayload::TaskExecutionSucceeded { .. }
                | EventPayload::TaskExecutionFailed { .. }
                | EventPayload::MergeConflictDetected { .. }
                | EventPayload::MergeCheckStarted { .. }
                | EventPayload::MergeCheckCompleted { .. }
                | EventPayload::RuntimeStarted { .. }
                | EventPayload::RuntimeEnvironmentPrepared { .. }
                | EventPayload::RuntimeCapabilitiesEvaluated { .. }
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
                | EventPayload::FileModified { .. }
                | EventPayload::CheckpointCommitCreated { .. }
                | EventPayload::ScopeValidated { .. }
                | EventPayload::ScopeViolationDetected { .. }
                | EventPayload::ScopeConflictDetected { .. }
                | EventPayload::TaskSchedulingDeferred { .. }
                | EventPayload::RetryContextAssembled { .. }
                | EventPayload::FlowIntegrationLockAcquired { .. }
                | EventPayload::WorktreeCleanupPerformed { .. }
                | EventPayload::WorktreeTurnRefRestored { .. }
                | EventPayload::Unknown
        )
    }

    pub(super) fn candidate_flow_ids_for_task(
        &self,
        flow_id: Option<Uuid>,
        task_id: &Uuid,
    ) -> Vec<Uuid> {
        let mut candidate_flow_ids = Vec::new();

        if let Some(fid) = flow_id {
            candidate_flow_ids.push(fid);
        } else {
            for (fid, flow) in &self.flows {
                if flow.task_executions.contains_key(task_id) {
                    candidate_flow_ids.push(*fid);
                }
            }
        }

        candidate_flow_ids
    }
}
