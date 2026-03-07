use super::*;

impl<'a> Scheduler<'a> {
    /// Creates a new scheduler.
    pub fn new(graph: &'a TaskGraph, flow: &'a mut TaskFlow) -> Self {
        Self { graph, flow }
    }

    /// Updates task readiness based on dependencies.
    pub fn update_readiness(&mut self) -> Vec<Uuid> {
        let mut newly_ready = Vec::new();

        for task_id in self.graph.tasks.keys() {
            if let Some(exec) = self.flow.get_task_execution(*task_id) {
                if exec.state != TaskExecState::Pending {
                    continue;
                }
            } else {
                continue;
            }

            if self.dependencies_satisfied(*task_id)
                && self
                    .flow
                    .transition_task(*task_id, TaskExecState::Ready)
                    .is_ok()
            {
                newly_ready.push(*task_id);
            }
        }

        newly_ready
    }

    fn dependencies_satisfied(&self, task_id: Uuid) -> bool {
        let Some(deps) = self.graph.dependencies.get(&task_id) else {
            return true;
        };

        for dep_id in deps {
            if let Some(exec) = self.flow.get_task_execution(*dep_id) {
                if exec.state != TaskExecState::Success {
                    return false;
                }
            } else {
                return false;
            }
        }

        true
    }

    /// Gets the current scheduling decision.
    pub fn schedule(&mut self) -> ScheduleResult {
        self.update_readiness();

        let failed: Vec<_> = self.flow.tasks_in_state(TaskExecState::Failed);
        let escalated: Vec<_> = self.flow.tasks_in_state(TaskExecState::Escalated);
        if !failed.is_empty() || !escalated.is_empty() {
            let mut failures = failed;
            failures.extend(escalated);
            return ScheduleResult::HasFailures(failures);
        }

        let ready: Vec<_> = self.flow.tasks_in_state(TaskExecState::Ready);
        if !ready.is_empty() {
            return ScheduleResult::Ready(ready);
        }

        let pending = self.flow.tasks_in_state(TaskExecState::Pending);
        let running = self.flow.tasks_in_state(TaskExecState::Running);
        let verifying = self.flow.tasks_in_state(TaskExecState::Verifying);
        let retry = self.flow.tasks_in_state(TaskExecState::Retry);

        if pending.is_empty() && running.is_empty() && verifying.is_empty() && retry.is_empty() {
            return ScheduleResult::Complete;
        }

        ScheduleResult::Blocked
    }

    /// Marks a task as started (Running).
    pub fn start_task(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        let exec = self
            .flow
            .get_task_execution(task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;

        if exec.state != TaskExecState::Ready && exec.state != TaskExecState::Retry {
            return Err(FlowError::InvalidTransition {
                from: exec.state,
                to: TaskExecState::Running,
            });
        }

        self.flow.transition_task(task_id, TaskExecState::Running)
    }

    /// Marks a task as completed execution (moves to Verifying).
    pub fn complete_execution(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        self.flow.transition_task(task_id, TaskExecState::Verifying)
    }

    /// Records verification result.
    pub fn record_verification(
        &mut self,
        task_id: Uuid,
        passed: bool,
        can_retry: bool,
    ) -> Result<(), FlowError> {
        let new_state = if passed {
            TaskExecState::Success
        } else if can_retry {
            TaskExecState::Retry
        } else {
            TaskExecState::Failed
        };

        self.flow.transition_task(task_id, new_state)
    }

    /// Escalates a failed task to human intervention.
    pub fn escalate(&mut self, task_id: Uuid) -> Result<(), FlowError> {
        let exec = self
            .flow
            .get_task_execution(task_id)
            .ok_or(FlowError::TaskNotFound(task_id))?;

        if exec.state != TaskExecState::Failed {
            return Err(FlowError::InvalidTransition {
                from: exec.state,
                to: TaskExecState::Escalated,
            });
        }

        self.flow.transition_task(task_id, TaskExecState::Escalated)
    }

    /// Gets tasks that are blocked by a specific task.
    pub fn blocked_by(&self, task_id: Uuid) -> Vec<Uuid> {
        self.graph.dependents(task_id)
    }

    /// Checks if the flow can complete.
    pub fn can_complete(&self) -> bool {
        let pending = self.flow.tasks_in_state(TaskExecState::Pending);
        let running = self.flow.tasks_in_state(TaskExecState::Running);
        let verifying = self.flow.tasks_in_state(TaskExecState::Verifying);
        let retry = self.flow.tasks_in_state(TaskExecState::Retry);
        let ready = self.flow.tasks_in_state(TaskExecState::Ready);

        pending.is_empty()
            && running.is_empty()
            && verifying.is_empty()
            && retry.is_empty()
            && ready.is_empty()
    }
}
