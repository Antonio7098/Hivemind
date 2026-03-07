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
}
