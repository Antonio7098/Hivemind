use super::*;

impl AppState {
    pub(super) fn apply_flow_lifecycle_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
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
            EventPayload::FlowFrozenForMerge { flow_id } => {
                if let Some(flow) = self.flows.get_mut(flow_id) {
                    flow.state = FlowState::FrozenForMerge;
                    flow.updated_at = timestamp;
                }
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
            _ => false,
        }
    }
}
