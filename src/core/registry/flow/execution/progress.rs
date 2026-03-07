use super::*;

impl Registry {
    pub(crate) fn auto_progress_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        const MAX_AUTO_ITERATIONS: usize = 1024;

        for _ in 0..MAX_AUTO_ITERATIONS {
            let latest = self.get_flow(flow_id)?;
            if latest.state != FlowState::Running {
                return Ok(latest);
            }

            let state = self.state()?;
            let graph = state.graphs.get(&latest.graph_id).ok_or_else(|| {
                HivemindError::system(
                    "graph_not_found",
                    "Graph not found",
                    "registry:auto_progress_flow",
                )
            })?;

            let has_verifying = !latest.tasks_in_state(TaskExecState::Verifying).is_empty();
            let has_auto_runnable = latest
                .tasks_in_state(TaskExecState::Retry)
                .into_iter()
                .chain(latest.tasks_in_state(TaskExecState::Ready))
                .any(|task_id| {
                    graph.tasks.contains_key(&task_id) && Self::can_auto_run_task(&state, task_id)
                });

            if !has_verifying && !has_auto_runnable {
                return Ok(latest);
            }

            let before_state = latest.state;
            let before_counts = latest.task_state_counts();
            let next = self.tick_flow(flow_id, false, None)?;
            let after_counts = next.task_state_counts();
            if before_state == next.state && before_counts == after_counts {
                return Ok(next);
            }
        }

        Err(HivemindError::system(
            "auto_progress_limit_exceeded",
            "Auto flow progression exceeded safety iteration limit",
            "registry:auto_progress_flow",
        ))
    }

    /// Closes a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found or already closed.
    pub fn close_task(&self, task_id: &str, reason: Option<&str>) -> Result<Task> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let state = self.state()?;
        let in_active_flow = state.flows.values().any(|f| {
            f.task_executions.contains_key(&task.id)
                && !matches!(
                    f.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if in_active_flow {
            let err = HivemindError::user(
                "task_in_active_flow",
                "Task is part of an active flow",
                "registry:close_task",
            );
            self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
            return Err(err);
        }

        if task.state == TaskState::Closed {
            // Idempotent: closing an already closed task is a no-op.
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskClosed {
                id: task.id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:close_task")
        })?;

        self.get_task(task_id)
    }
}
