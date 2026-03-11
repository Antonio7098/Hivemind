use super::*;
use crate::core::workflow::{WorkflowRunState, WorkflowStepState};

impl AppState {
    pub(super) fn apply_workflow_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::WorkflowDefinitionCreated { definition }
            | EventPayload::WorkflowDefinitionUpdated { definition } => {
                self.workflows.insert(definition.id, definition.clone());
                true
            }
            EventPayload::WorkflowRunCreated { run } => {
                self.workflow_runs.insert(run.id, run.clone());
                true
            }
            EventPayload::WorkflowRunStarted { workflow_run_id } => {
                self.apply_run_state_update(workflow_run_id, WorkflowRunState::Running, timestamp)
            }
            EventPayload::WorkflowRunPaused { workflow_run_id } => {
                self.set_workflow_run_state(workflow_run_id, WorkflowRunState::Paused, timestamp)
            }
            EventPayload::WorkflowRunResumed { workflow_run_id } => {
                self.set_workflow_run_state(workflow_run_id, WorkflowRunState::Running, timestamp)
            }
            EventPayload::WorkflowRunCompleted { workflow_run_id } => {
                self.set_workflow_run_state(workflow_run_id, WorkflowRunState::Completed, timestamp)
            }
            EventPayload::WorkflowRunAborted {
                workflow_run_id,
                reason,
                ..
            } => self.apply_run_abort(workflow_run_id, reason.as_ref(), timestamp),
            EventPayload::WorkflowStepStateChanged {
                workflow_run_id,
                step_id,
                step_run_id,
                state,
                reason,
            } => self.apply_step_state_change(
                workflow_run_id,
                step_id,
                step_run_id,
                *state,
                reason.as_ref(),
                timestamp,
            ),
            _ => false,
        }
    }

    fn set_workflow_run_state(
        &mut self,
        workflow_run_id: &Uuid,
        state: WorkflowRunState,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        run.state = state;
        if matches!(
            state,
            WorkflowRunState::Completed | WorkflowRunState::Aborted
        ) {
            run.completed_at = Some(timestamp);
        }
        run.updated_at = timestamp;
        true
    }

    fn apply_run_state_update(
        &mut self,
        workflow_run_id: &Uuid,
        state: WorkflowRunState,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        run.state = state;
        run.started_at.get_or_insert(timestamp);
        run.updated_at = timestamp;

        if let Some(definition) = self.workflows.get(&run.workflow_id) {
            for step_id in definition.root_step_ids() {
                if let Some(step_run) = run.step_runs.get_mut(&step_id) {
                    if step_run.state == WorkflowStepState::Pending {
                        step_run.state = WorkflowStepState::Ready;
                        step_run.updated_at = timestamp;
                    }
                }
            }
        }
        true
    }

    fn apply_run_abort(
        &mut self,
        workflow_run_id: &Uuid,
        reason: Option<&String>,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        run.state = WorkflowRunState::Aborted;
        run.completed_at = Some(timestamp);
        run.updated_at = timestamp;

        if let Some(abort_reason) = reason {
            for step_run in run.step_runs.values_mut() {
                if matches!(
                    step_run.state,
                    WorkflowStepState::Pending | WorkflowStepState::Ready
                ) {
                    step_run.state = WorkflowStepState::Aborted;
                    step_run.reason = Some(abort_reason.clone());
                    step_run.updated_at = timestamp;
                }
            }
        }
        true
    }

    fn apply_step_state_change(
        &mut self,
        workflow_run_id: &Uuid,
        step_id: &Uuid,
        step_run_id: &Uuid,
        state: WorkflowStepState,
        reason: Option<&String>,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        let Some(step_run) = run.step_runs.get_mut(step_id) else {
            return false;
        };
        if step_run.id != *step_run_id {
            return false;
        }
        step_run.state = state;
        step_run.reason.clone_from(&reason.cloned());
        step_run.updated_at = timestamp;
        run.updated_at = timestamp;
        true
    }
}
