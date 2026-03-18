use super::*;

mod check_runner;
mod outcomes;
mod scope_validation;

impl Registry {
    // ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn process_verifying_task(&self, flow_id: &str, task_id: Uuid) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Ok(flow);
        }

        let state = self.state()?;
        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
        })?;

        let origin = "registry:tick_flow";
        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Verifying {
            return Ok(flow);
        }

        let attempt = Self::resolve_latest_attempt_with_diff(&state, flow.id, task_id, origin)?;

        let worktree_status = Self::inspect_task_worktree(&flow, &state, task_id, origin)?;

        let task = graph.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;

        let verification = self.verify_task_scope_artifacts(
            &flow,
            &state,
            task_id,
            task,
            &attempt,
            &worktree_status,
            origin,
        )?;

        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);

        if !verification.passed {
            self.handle_scope_violation(
                &flow,
                task_id,
                attempt.id,
                &verification,
                task,
                &worktree_status,
                origin,
            )?;
            unreachable!("handle_scope_violation always returns Err");
        }

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt.id,
        );

        if let Some(scope) = &task.scope {
            self.append_event(
                Event::new(
                    EventPayload::ScopeValidated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        verification_id: verification.id,
                        verified_at: verification.verified_at,
                        scope: scope.clone(),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        let (checks_passed, results) = self.run_verification_checks(
            &flow,
            task_id,
            task,
            &attempt,
            &worktree_status,
            &corr_attempt,
            origin,
        )?;

        if checks_passed {
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        from: TaskExecState::Verifying,
                        to: TaskExecState::Success,
                    },
                    corr_task,
                ),
                origin,
            )?;

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionSucceeded {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                    },
                    corr_attempt,
                ),
                origin,
            )?;

            let frozen_commit_sha = Self::resolve_task_frozen_commit_sha(&flow, &state, task_id);
            self.emit_task_execution_frozen(&flow, task_id, frozen_commit_sha, origin)?;

            if let Ok(managers) =
                Self::worktree_managers_for_flow(&flow, &state, "registry:tick_flow")
            {
                for (_repo_name, manager) in managers {
                    if manager.config().cleanup_on_success {
                        if let Ok(status) = manager.inspect(flow.id, task_id) {
                            if status.is_worktree {
                                let _ = manager.remove(&status.path);
                            }
                        }
                    }
                }
            }

            let updated = self.get_flow(flow_id)?;
            let all_success = updated
                .task_executions
                .values()
                .all(|e| e.state == TaskExecState::Success);
            if all_success {
                let event = Event::new(
                    EventPayload::TaskFlowCompleted {
                        flow_id: updated.id,
                    },
                    CorrelationIds::for_graph_flow(
                        updated.project_id,
                        updated.graph_id,
                        updated.id,
                    ),
                );
                self.append_event(event, origin)?;
                self.maybe_autostart_dependent_flows(updated.id)?;
            } else if updated.run_mode == RunMode::Auto {
                return self.tick_flow(&updated.id.to_string(), false, None);
            }

            return self.get_flow(flow_id);
        }

        self.handle_verification_failure(
            &flow,
            task_id,
            attempt.id,
            task,
            exec,
            &results,
            &worktree_status,
            origin,
        )?;
        unreachable!("handle_verification_failure always returns Err");
    }
}
