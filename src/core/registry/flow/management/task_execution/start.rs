use super::*;

impl Registry {
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
}
