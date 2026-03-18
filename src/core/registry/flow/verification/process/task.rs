use super::*;

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
            if let Some(scope) = &task.scope {
                self.append_event(
                    Event::new(
                        EventPayload::ScopeViolationDetected {
                            flow_id: flow.id,
                            task_id,
                            attempt_id: attempt.id,
                            verification_id: verification.id,
                            verified_at: verification.verified_at,
                            scope: scope.clone(),
                            violations: verification.violations.clone(),
                        },
                        CorrelationIds::for_graph_flow_task_attempt(
                            flow.project_id,
                            flow.graph_id,
                            flow.id,
                            task_id,
                            attempt.id,
                        ),
                    ),
                    origin,
                )?;
            }

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        from: TaskExecState::Verifying,
                        to: TaskExecState::Failed,
                    },
                    corr_task,
                ),
                origin,
            )?;

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        reason: Some("scope_violation".to_string()),
                    },
                    CorrelationIds::for_graph_flow_task_attempt(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        task_id,
                        attempt.id,
                    ),
                ),
                origin,
            )?;

            let violations = verification
                .violations
                .iter()
                .map(|v| {
                    let path = v.path.as_deref().unwrap_or("-");
                    format!("{:?}: {path}: {}", v.violation_type, v.description)
                })
                .collect::<Vec<_>>()
                .join("\n");

            return Err(HivemindError::scope(
                "scope_violation",
                format!("Scope violation detected:\n{violations}"),
                origin,
            )
            .with_hint(format!(
                "Worktree preserved at {}",
                worktree_status.path.display()
            )));
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

        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt.id.to_string())
            .join("checks");
        let _ = fs::create_dir_all(&target_dir);

        let mut results = Vec::new();
        for check in &task.criteria.checks {
            self.append_event(
                Event::new(
                    EventPayload::CheckStarted {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        check_name: check.name.clone(),
                        required: check.required,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;

            let started = Instant::now();
            let (exit_code, combined) = match Self::run_check_command(
                &worktree_status.path,
                &target_dir,
                &check.command,
                check.timeout_ms,
            ) {
                Ok((exit_code, output, _timed_out)) => (exit_code, output),
                Err(e) => (127, e.to_string()),
            };
            let duration_ms =
                u64::try_from(started.elapsed().as_millis().min(u128::from(u64::MAX)))
                    .unwrap_or(u64::MAX);
            let passed = exit_code == 0;

            self.append_event(
                Event::new(
                    EventPayload::CheckCompleted {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        check_name: check.name.clone(),
                        passed,
                        exit_code,
                        output: combined.clone(),
                        duration_ms,
                        required: check.required,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;

            results.push((check.name.clone(), check.required, passed));
        }

        let required_failed = results
            .iter()
            .any(|(_, required, passed)| *required && !*passed);

        if !required_failed {
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

        let max_retries = task.retry_policy.max_retries;
        let max_attempts = max_retries.saturating_add(1);
        let can_retry = exec.attempt_count < max_attempts;
        let to = if can_retry {
            TaskExecState::Retry
        } else {
            TaskExecState::Failed
        };

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt.id),
                    from: TaskExecState::Verifying,
                    to,
                },
                corr_task,
            ),
            origin,
        )?;

        if matches!(to, TaskExecState::Retry | TaskExecState::Failed) {
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        reason: Some("required_checks_failed".to_string()),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        let failures = results
            .into_iter()
            .filter(|(_, required, passed)| *required && !*passed)
            .map(|(name, _, _)| name)
            .collect::<Vec<_>>()
            .join(", ");

        let err = HivemindError::verification(
            "required_checks_failed",
            format!("Required checks failed: {failures}"),
            origin,
        )
        .with_hint(format!(
            "View check outputs via `hivemind verify results {}`. Worktree preserved at {}",
            attempt.id,
            worktree_status.path.display()
        ));

        self.record_error_event(&err, corr_attempt);

        Err(err)
    }
}
