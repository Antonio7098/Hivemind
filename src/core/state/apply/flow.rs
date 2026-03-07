use super::*;

impl AppState {
    #[allow(clippy::too_many_lines)]
    pub(super) fn apply_flow_event(&mut self, event: &Event, timestamp: DateTime<Utc>) -> bool {
        match &event.payload {
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id,
                project_id,
                name: _,
                task_ids,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }

                let mut task_executions = HashMap::new();
                for task_id in task_ids {
                    task_executions.insert(
                        *task_id,
                        TaskExecution {
                            task_id: *task_id,
                            state: TaskExecState::Pending,
                            attempt_count: 0,
                            retry_mode: RetryMode::default(),
                            frozen_commit_sha: None,
                            integrated_commit_sha: None,
                            updated_at: timestamp,
                            blocked_reason: None,
                        },
                    );
                }

                self.flows.insert(
                    *flow_id,
                    TaskFlow {
                        id: *flow_id,
                        graph_id: *graph_id,
                        project_id: *project_id,
                        base_revision: None,
                        run_mode: RunMode::Manual,
                        depends_on_flows: HashSet::new(),
                        state: FlowState::Created,
                        task_executions,
                        created_at: timestamp,
                        started_at: None,
                        completed_at: None,
                        updated_at: timestamp,
                    },
                );
                self.flow_runtime_defaults.entry(*flow_id).or_default();
                true
            }
            EventPayload::TaskFlowDependencyAdded {
                flow_id,
                depends_on_flow_id,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.depends_on_flows.insert(*depends_on_flow_id);
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowRunModeSet { flow_id, mode } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.run_mode = *mode;
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowRuntimeConfigured {
                flow_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                let configured = ProjectRuntimeConfig {
                    adapter_name: adapter_name.clone(),
                    binary_path: binary_path.clone(),
                    model: model.clone(),
                    args: args.clone(),
                    env: env.clone(),
                    timeout_ms: *timeout_ms,
                    max_parallel_tasks: *max_parallel_tasks,
                };
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, Some(configured));
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowRuntimeCleared { flow_id, role } => {
                self.flow_runtime_defaults
                    .entry(*flow_id)
                    .or_default()
                    .set(*role, None);
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowStarted {
                flow_id,
                base_revision,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.started_at = Some(timestamp);
                    flow.base_revision.clone_from(base_revision);
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowPaused {
                flow_id,
                running_tasks: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Paused;
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowResumed { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Running;
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowCompleted { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Completed;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::FlowFrozenForMerge { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::FrozenForMerge;
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowAborted {
                flow_id,
                reason: _,
                forced: _,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::Aborted;
                    flow.completed_at = Some(timestamp);
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskFlowDeleted {
                flow_id,
                graph_id: _,
                project_id: _,
            } => {
                self.flows.remove(flow_id);
                self.flow_runtime_defaults.remove(flow_id);
                self.merge_states.remove(flow_id);
                self.attempts
                    .retain(|_, attempt| attempt.flow_id != *flow_id);
                true
            }
            EventPayload::TaskReady { flow_id, task_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Ready;
                        exec.blocked_reason = None;
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskBlocked {
                flow_id,
                task_id,
                reason,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = TaskExecState::Pending;
                        exec.blocked_reason.clone_from(reason);
                        exec.updated_at = timestamp;
                    }
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskExecutionStateChanged {
                flow_id,
                task_id,
                attempt_id: _,
                from: _,
                to,
            } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    if let Some(exec) = flow.task_executions.get_mut(task_id) {
                        exec.state = *to;
                        exec.updated_at = timestamp;
                        exec.blocked_reason = None;
                        if *to == TaskExecState::Running {
                            exec.attempt_count += 1;
                        }
                    }
                    flow.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRetryRequested {
                task_id,
                reset_count,
                retry_mode,
            } => {
                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Pending;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            exec.retry_mode = *retry_mode;
                            if *reset_count {
                                exec.attempt_count = 0;
                            }
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
                true
            }
            EventPayload::TaskAborted { task_id, reason: _ } => {
                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = TaskExecState::Failed;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
                true
            }
            EventPayload::HumanOverride {
                task_id,
                override_type: _,
                decision,
                reason: _,
                user: _,
            } => {
                let new_state = if decision == "pass" {
                    TaskExecState::Success
                } else {
                    TaskExecState::Failed
                };

                for fid in
                    self.candidate_flow_ids_for_task(event.metadata.correlation.flow_id, task_id)
                {
                    if let Some(flow) = self.flows.get_mut(&fid) {
                        if let Some(exec) = flow.task_executions.get_mut(task_id) {
                            exec.state = new_state;
                            exec.blocked_reason = None;
                            exec.updated_at = timestamp;
                            flow.updated_at = timestamp;
                            break;
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }
}
