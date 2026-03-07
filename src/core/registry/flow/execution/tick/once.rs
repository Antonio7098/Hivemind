use super::*;
mod attempt;
mod readiness;
mod runtime;
mod worktree;

impl Registry {
    pub(crate) fn tick_flow_once(
        &self,
        flow_id: &str,
        interactive: bool,
        preferred_task: Option<Uuid>,
    ) -> Result<TaskFlow> {
        let origin = "registry:tick_flow";
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                origin,
            ));
        }

        let state = self.state()?;
        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;

        let mut verifying = flow.tasks_in_state(TaskExecState::Verifying);
        verifying.sort();
        if let Some(task_id) = verifying.first().copied() {
            return self.process_verifying_task(flow_id, task_id);
        }

        self.process_pending_task_transitions(&flow, graph, origin)?;

        let flow = self.get_flow(flow_id)?;
        let task_to_run = Self::select_task_to_run(&state, &flow, preferred_task);
        let Some(task_id) = task_to_run else {
            if let Some(updated_flow) = self.maybe_complete_finished_flow(&flow, flow_id, origin)? {
                return Ok(updated_flow);
            }
            return Ok(flow);
        };

        let launch_prereqs = self.prepare_tick_launch_prereqs(&state, &flow, task_id, origin)?;
        self.merge_task_dependency_branches(
            &state,
            &flow,
            graph,
            task_id,
            &launch_prereqs.repo_worktrees,
            origin,
        )?;

        let attempt_launch = self.start_tick_attempt(
            &state,
            &flow,
            graph,
            task_id,
            &launch_prereqs.runtime,
            &launch_prereqs.repo_worktrees,
            launch_prereqs.next_attempt_number,
            origin,
        )?;

        let attempt_id = attempt_launch.attempt_id;
        let attempt_corr = attempt_launch.attempt_corr;
        let input = attempt_launch.input;
        let runtime_prompt = attempt_launch.runtime_prompt;
        let runtime_flags = attempt_launch.runtime_flags;
        let task_scope = attempt_launch.task_scope;
        let max_attempts = attempt_launch.max_attempts;

        let Some(runtime_for_adapter) = self.prepare_runtime_for_tick_attempt(
            &state,
            &flow,
            task_id,
            &launch_prereqs.worktree_status,
            &launch_prereqs.repo_worktrees,
            launch_prereqs.runtime,
            launch_prereqs.runtime_selection_source,
            task_scope,
            attempt_id,
            &attempt_corr,
            launch_prereqs.next_attempt_number,
            max_attempts,
            runtime_flags,
            runtime_prompt,
            origin,
        )?
        else {
            return self.get_flow(flow_id);
        };

        let Some(execution) = self.execute_tick_attempt(
            interactive,
            &state,
            &flow,
            task_id,
            &launch_prereqs.worktree_status,
            runtime_for_adapter,
            input,
            attempt_id,
            &attempt_corr,
            launch_prereqs.next_attempt_number,
            max_attempts,
            origin,
        )?
        else {
            return self.get_flow(flow_id);
        };

        if execution.report.exit_code != 0 {
            let (failure_code, failure_message, recoverable) = execution
                .report
                .errors
                .first()
                .filter(|error| error.code.starts_with("native_"))
                .map_or_else(
                    || {
                        (
                            "runtime_nonzero_exit".to_string(),
                            format!("Runtime exited with code {}", execution.report.exit_code),
                            true,
                        )
                    },
                    |error| (error.code.clone(), error.message.clone(), error.recoverable),
                );
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &execution.runtime_for_adapter,
                launch_prereqs.next_attempt_number,
                max_attempts,
                &failure_code,
                &failure_message,
                recoverable,
                &execution.report.stdout,
                &execution.report.stderr,
                origin,
            )?;
            return self.get_flow(flow_id);
        }

        if let Err(err) = self.complete_task_execution(&task_id.to_string()) {
            if err.code == "checkpoints_incomplete" {
                if let Some((failure_code, failure_message, recoverable)) =
                    Self::detect_runtime_output_failure(
                        &execution.report.stdout,
                        &execution.report.stderr,
                    )
                {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &execution.runtime_for_adapter,
                        launch_prereqs.next_attempt_number,
                        max_attempts,
                        &failure_code,
                        &failure_message,
                        recoverable,
                        &execution.report.stdout,
                        &execution.report.stderr,
                        origin,
                    )?;
                    return self.get_flow(flow_id);
                }

                self.handle_runtime_failure(
                    &state,
                    &flow,
                    task_id,
                    attempt_id,
                    &execution.runtime_for_adapter,
                    launch_prereqs.next_attempt_number,
                    max_attempts,
                    "checkpoints_incomplete",
                    &err.message,
                    true,
                    &execution.report.stdout,
                    &execution.report.stderr,
                    origin,
                )?;
                return Err(err);
            }
            return Err(err);
        }

        self.process_verifying_task(flow_id, task_id)
    }
}
