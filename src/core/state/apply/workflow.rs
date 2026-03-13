use super::*;
use crate::core::workflow::{
    WorkflowContextSnapshot, WorkflowContextState, WorkflowOutputBagEntry, WorkflowRunState,
    WorkflowSignal, WorkflowStepContextSnapshot, WorkflowStepState, WorkflowWaitStatus,
};

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
            EventPayload::WorkflowConditionEvaluated { .. } => true,
            EventPayload::WorkflowWaitActivated {
                workflow_run_id,
                step_id,
                step_run_id,
                wait_status,
            }
            | EventPayload::WorkflowWaitCompleted {
                workflow_run_id,
                step_id,
                step_run_id,
                wait_status,
            } => self.apply_wait_status(
                workflow_run_id,
                step_id,
                step_run_id,
                Some(wait_status.clone()),
                timestamp,
            ),
            EventPayload::WorkflowSignalReceived {
                workflow_run_id,
                signal,
            } => self.apply_workflow_signal(workflow_run_id, signal.clone(), timestamp),
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
            EventPayload::WorkflowContextInitialized {
                workflow_run_id,
                context,
            } => self.apply_workflow_context_initialization(
                workflow_run_id,
                context.clone(),
                timestamp,
            ),
            EventPayload::WorkflowContextSnapshotCaptured {
                workflow_run_id,
                snapshot,
            } => self.apply_workflow_context_snapshot(workflow_run_id, snapshot.clone(), timestamp),
            EventPayload::WorkflowStepInputsResolved {
                workflow_run_id,
                step_id,
                step_run_id,
                snapshot,
            } => self.apply_step_inputs_resolved(
                workflow_run_id,
                step_id,
                step_run_id,
                snapshot.clone(),
                timestamp,
            ),
            EventPayload::WorkflowOutputAppended {
                workflow_run_id,
                step_id,
                step_run_id,
                entry,
            } => self.apply_workflow_output_append(
                workflow_run_id,
                step_id,
                step_run_id,
                entry.clone(),
                timestamp,
            ),
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
        if state != WorkflowStepState::Waiting {
            step_run.wait_status = None;
        }
        step_run.updated_at = timestamp;
        run.updated_at = timestamp;
        true
    }

    fn apply_wait_status(
        &mut self,
        workflow_run_id: &Uuid,
        step_id: &Uuid,
        step_run_id: &Uuid,
        wait_status: Option<WorkflowWaitStatus>,
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
        step_run.wait_status = wait_status;
        step_run.updated_at = timestamp;
        run.updated_at = timestamp;
        true
    }

    fn apply_workflow_signal(
        &mut self,
        workflow_run_id: &Uuid,
        signal: WorkflowSignal,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        if run
            .signals
            .iter()
            .all(|existing| existing.idempotency_key != signal.idempotency_key)
        {
            run.signals.push(signal);
            run.updated_at = timestamp;
        }
        true
    }

    fn apply_workflow_context_initialization(
        &mut self,
        workflow_run_id: &Uuid,
        context: WorkflowContextState,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        run.context = context;
        run.updated_at = timestamp;
        true
    }

    fn apply_workflow_context_snapshot(
        &mut self,
        workflow_run_id: &Uuid,
        snapshot: WorkflowContextSnapshot,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        if let Some(existing) = run
            .context
            .snapshots
            .iter_mut()
            .find(|item| item.revision == snapshot.revision)
        {
            *existing = snapshot.clone();
        } else {
            run.context.snapshots.push(snapshot.clone());
            run.context.snapshots.sort_by(|a, b| {
                a.revision
                    .cmp(&b.revision)
                    .then_with(|| a.created_at.cmp(&b.created_at))
            });
        }
        run.context.current_snapshot = snapshot;
        run.updated_at = timestamp;
        true
    }

    fn apply_step_inputs_resolved(
        &mut self,
        workflow_run_id: &Uuid,
        step_id: &Uuid,
        step_run_id: &Uuid,
        snapshot: WorkflowStepContextSnapshot,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        let Some(step_run) = run.step_runs.get(step_id) else {
            return false;
        };
        if step_run.id != *step_run_id {
            return false;
        }
        run.step_contexts.insert(*step_id, snapshot);
        run.updated_at = timestamp;
        true
    }

    fn apply_workflow_output_append(
        &mut self,
        workflow_run_id: &Uuid,
        step_id: &Uuid,
        step_run_id: &Uuid,
        entry: WorkflowOutputBagEntry,
        timestamp: DateTime<Utc>,
    ) -> bool {
        let Some(run) = self.workflow_runs.get_mut(workflow_run_id) else {
            return false;
        };
        let Some(step_run) = run.step_runs.get(step_id) else {
            return false;
        };
        if step_run.id != *step_run_id {
            return false;
        }
        run.output_bag = run.output_bag.append(entry);
        run.updated_at = timestamp;
        true
    }
}
