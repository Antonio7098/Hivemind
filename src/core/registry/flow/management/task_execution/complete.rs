use super::*;

impl Registry {
    pub fn complete_task_execution(&self, task_id: &str) -> Result<TaskFlow> {
        let origin = "registry:complete_task_execution";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;
        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Running {
            return Err(HivemindError::user(
                "task_not_running",
                "Task is not in running state",
                origin,
            ));
        }

        let attempt = Self::resolve_latest_attempt_without_diff(&state, flow.id, id, origin)?;

        if !attempt.all_checkpoints_completed {
            let err = HivemindError::user(
                    "checkpoints_incomplete",
                    "All checkpoints must be completed before task completion",
                    origin,
                )
                .with_hint(format!(
                    "Complete the active checkpoint via `hivemind checkpoint complete --attempt-id {} --id <checkpoint-id>` before finishing the task attempt",
                    attempt.id
                ));
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow_task_attempt(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    id,
                    attempt.id,
                ),
            );
            return Err(err);
        }

        let baseline_id = attempt.baseline_id.ok_or_else(|| {
            HivemindError::system(
                "baseline_not_found",
                "Baseline not found for attempt",
                origin,
            )
        })?;

        let status = Self::inspect_task_worktree(&flow, &state, id, origin)?;
        let artifact =
            self.compute_and_store_diff(baseline_id, &status.path, id, attempt.id, origin)?;

        self.emit_task_execution_completion_events(
            &flow,
            id,
            &attempt,
            CompletionArtifacts {
                baseline_id,
                artifact: &artifact,
                checkpoint_commit_sha: None,
            },
            origin,
        )?;

        let updated = self.get_flow(&flow.id.to_string())?;
        if updated.run_mode == RunMode::Auto {
            return self.auto_progress_flow(&flow.id.to_string());
        }
        Ok(updated)
    }
}
