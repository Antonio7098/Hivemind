use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn retry_task(
        &self,
        task_id: &str,
        reset_count: bool,
        retry_mode: RetryMode,
    ) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:retry_task",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let mut candidates: Vec<TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&id))
            .cloned()
            .collect();

        if candidates.is_empty() {
            let err = HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:retry_task",
            );
            let corr = state
                .tasks
                .get(&id)
                .map_or_else(CorrelationIds::none, |task| {
                    CorrelationIds::for_task(task.project_id, task.id)
                });
            self.record_error_event(&err, corr);
            return Err(err);
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:retry_task",
            )
        })?;

        let max_retries = state
            .graphs
            .get(&flow.graph_id)
            .and_then(|g| g.tasks.get(&id))
            .map_or(3, |t| t.retry_policy.max_retries);
        let max_attempts = max_retries.saturating_add(1);
        if !reset_count && exec.attempt_count >= max_attempts {
            let err = HivemindError::user(
                "retry_limit_exceeded",
                "Retry limit exceeded",
                "registry:retry_task",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
            );
            return Err(err);
        }

        if exec.state != TaskExecState::Failed && exec.state != TaskExecState::Retry {
            let err = HivemindError::user(
                "task_not_retriable",
                "Task is not in a retriable state",
                "registry:retry_task",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
            );
            return Err(err);
        }

        if matches!(retry_mode, RetryMode::Clean) {
            if let Ok(managers) =
                Self::worktree_managers_for_flow(&flow, &state, "registry:retry_task")
            {
                for (idx, (_repo_name, manager)) in managers.iter().enumerate() {
                    let base = Self::default_base_ref_for_repo(&flow, manager, idx == 0);
                    let mut status = manager.inspect(flow.id, id).ok();
                    if status.as_ref().is_none_or(|s| !s.is_worktree) {
                        let _ = manager.create(flow.id, id, Some(&base));
                        status = manager.inspect(flow.id, id).ok();
                    }

                    if let Some(status) = status.filter(|s| s.is_worktree) {
                        let branch = format!("exec/{}/{id}", flow.id);
                        Self::checkout_and_clean_worktree(
                            &status.path,
                            &branch,
                            &base,
                            "registry:retry_task",
                        )?;
                    }
                }
            }
        }

        let event = Event::new(
            EventPayload::TaskRetryRequested {
                task_id: id,
                reset_count,
                retry_mode,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:retry_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn start_task_execution(&self, task_id: &str) -> Result<Uuid> {
        let origin = "registry:start_task_execution";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let flow = match Self::flow_for_task(&state, id, origin) {
            Ok(flow) => flow,
            Err(err) => {
                let corr = state
                    .tasks
                    .get(&id)
                    .map_or_else(CorrelationIds::none, |task| {
                        CorrelationIds::for_task(task.project_id, task.id)
                    });
                self.record_error_event(&err, corr);
                return Err(err);
            }
        };
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id);
        if flow.state != FlowState::Running {
            let err =
                HivemindError::user("flow_not_running", "Flow is not in running state", origin);
            self.record_error_event(&err, corr_task);
            return Err(err);
        }

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Ready && exec.state != TaskExecState::Retry {
            let err = HivemindError::user("task_not_ready", "Task is not ready to start", origin);
            self.record_error_event(&err, corr_task);
            return Err(err);
        }

        let status = Self::ensure_task_worktree(&flow, &state, id, origin)?;
        let attempt_id = Uuid::new_v4();
        let attempt_number = exec.attempt_count.saturating_add(1);
        let baseline = self.capture_and_store_baseline(&status.path, origin)?;

        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;
        let graph_task = graph.tasks.get(&id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;
        let checkpoint_ids = Self::normalized_checkpoint_ids(&graph_task.checkpoints);
        if graph_task.scope.is_some() {
            self.capture_scope_baseline_for_attempt(&flow, &state, attempt_id)?;
        }

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id: Some(attempt_id),
                    from: exec.state,
                    to: TaskExecState::Running,
                },
                corr_task,
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::AttemptStarted {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    attempt_number,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let total = u32::try_from(checkpoint_ids.len()).map_err(|_| {
            HivemindError::system(
                "checkpoint_count_overflow",
                "Checkpoint count exceeds supported range",
                origin,
            )
        })?;

        for (idx, checkpoint_id) in checkpoint_ids.iter().enumerate() {
            let order = u32::try_from(idx.saturating_add(1)).map_err(|_| {
                HivemindError::system(
                    "checkpoint_order_overflow",
                    "Checkpoint order exceeds supported range",
                    origin,
                )
            })?;

            self.append_event(
                Event::new(
                    EventPayload::CheckpointDeclared {
                        flow_id: flow.id,
                        task_id: id,
                        attempt_id,
                        checkpoint_id: checkpoint_id.clone(),
                        order,
                        total,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        if let Some(first_checkpoint_id) = checkpoint_ids.first() {
            self.append_event(
                Event::new(
                    EventPayload::CheckpointActivated {
                        flow_id: flow.id,
                        task_id: id,
                        attempt_id,
                        checkpoint_id: first_checkpoint_id.clone(),
                        order: 1,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStarted {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    attempt_number,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::BaselineCaptured {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    baseline_id: baseline.id,
                    git_head: baseline.git_head.clone(),
                    file_count: baseline.file_count(),
                },
                corr_attempt,
            ),
            origin,
        )?;

        Ok(attempt_id)
    }

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

    pub fn abort_task(&self, task_id: &str, reason: Option<&str>) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:abort_task",
            )
        })?;

        let state = self.state()?;
        let mut candidates: Vec<TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&id))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:abort_task",
            ));
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:abort_task",
            )
        })?;

        if exec.state == TaskExecState::Success {
            return Err(HivemindError::user(
                "task_already_terminal",
                "Task is already successful",
                "registry:abort_task",
            ));
        }

        if exec.state == TaskExecState::Failed {
            return Ok(flow);
        }

        let event = Event::new(
            EventPayload::TaskAborted {
                task_id: id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_task")
        })?;

        let event = Event::new(
            EventPayload::TaskExecutionFailed {
                flow_id: flow.id,
                task_id: id,
                attempt_id: None,
                reason: Some("aborted".to_string()),
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn verify_override(&self, task_id: &str, decision: &str, reason: &str) -> Result<TaskFlow> {
        let origin = "registry:verify_override";

        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        if decision != "pass" && decision != "fail" {
            return Err(HivemindError::user(
                "invalid_decision",
                "Decision must be 'pass' or 'fail'",
                origin,
            ));
        }

        if reason.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_reason",
                "Reason must be non-empty",
                origin,
            ));
        }

        let user = env::var("HIVEMIND_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty());

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;

        if matches!(
            flow.state,
            FlowState::Completed
                | FlowState::FrozenForMerge
                | FlowState::Merged
                | FlowState::Aborted
        ) {
            return Err(HivemindError::user(
                "flow_not_active",
                "Cannot override verification for a completed or aborted flow",
                origin,
            ));
        }

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;

        // Allow overrides both during verification and after automated decisions.
        // This supports overriding failed checks/verifier outcomes (Retry/Failed/Escalated).
        if !matches!(
            exec.state,
            TaskExecState::Verifying
                | TaskExecState::Retry
                | TaskExecState::Failed
                | TaskExecState::Escalated
        ) {
            return Err(HivemindError::user(
                "task_not_overridable",
                "Task is not in an overridable state",
                origin,
            ));
        }

        let event = Event::new(
            EventPayload::HumanOverride {
                task_id: id,
                override_type: "VERIFICATION_OVERRIDE".to_string(),
                decision: decision.to_string(),
                reason: reason.to_string(),
                user,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        let updated = self.get_flow(&flow.id.to_string())?;

        if decision == "pass" {
            let frozen_commit_sha = Self::resolve_task_frozen_commit_sha(&updated, &state, id);
            self.emit_task_execution_frozen(&updated, id, frozen_commit_sha, origin)?;

            if let Ok(managers) =
                Self::worktree_managers_for_flow(&updated, &state, "registry:verify_override")
            {
                for (_repo_name, manager) in managers {
                    if manager.config().cleanup_on_success {
                        if let Ok(status) = manager.inspect(updated.id, id) {
                            if status.is_worktree {
                                let _ = manager.remove(&status.path);
                            }
                        }
                    }
                }
            }

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
                let _ = self.store.append(event);
            }
        }

        self.get_flow(&flow.id.to_string())
    }
}
