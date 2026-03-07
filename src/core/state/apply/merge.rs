use super::*;

impl AppState {
    pub(super) fn apply_merge_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::TaskExecutionFrozen {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.frozen_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
                true
            }
            EventPayload::TaskIntegratedIntoFlow {
                flow_id,
                task_id,
                commit_sha,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.integrated_commit_sha.clone_from(commit_sha);
                        exec.updated_at = timestamp;
                        flow.updated_at = timestamp;
                    }
                }
                true
            }
            EventPayload::MergePrepared {
                flow_id,
                target_branch,
                conflicts,
            } => {
                self.merge_states.insert(
                    *flow_id,
                    MergeState {
                        flow_id: *flow_id,
                        status: MergeStatus::Prepared,
                        target_branch: target_branch.clone(),
                        conflicts: conflicts.clone(),
                        commits: Vec::new(),
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::MergeApproved { flow_id, user: _ } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Approved;
                    ms.updated_at = timestamp;
                }
                true
            }
            EventPayload::MergeCompleted { flow_id, commits } => {
                if let Some(ms) = self.merge_states.get_mut(flow_id) {
                    ms.status = MergeStatus::Completed;
                    commits.clone_into(&mut ms.commits);
                    ms.updated_at = timestamp;
                }

                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Merged;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
