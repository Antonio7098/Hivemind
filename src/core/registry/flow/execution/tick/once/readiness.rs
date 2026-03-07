use super::*;

impl Registry {
    pub(super) fn process_pending_task_transitions(
        &self,
        flow: &TaskFlow,
        graph: &TaskGraph,
        origin: &'static str,
    ) -> Result<()> {
        let mut newly_ready = Vec::new();
        let mut newly_blocked: Vec<(Uuid, String)> = Vec::new();
        for task_id in graph.tasks.keys() {
            let Some(exec) = flow.task_executions.get(task_id) else {
                continue;
            };
            if exec.state != TaskExecState::Pending {
                continue;
            }

            let deps_satisfied = graph.dependencies.get(task_id).is_none_or(|deps| {
                deps.iter().all(|dep| {
                    flow.task_executions
                        .get(dep)
                        .is_some_and(|e| e.state == TaskExecState::Success)
                })
            });

            if deps_satisfied {
                newly_ready.push(*task_id);
            } else {
                let mut missing: Vec<Uuid> = graph
                    .dependencies
                    .get(task_id)
                    .map(|deps| {
                        deps.iter()
                            .filter(|dep| {
                                flow.task_executions
                                    .get(dep)
                                    .is_none_or(|e| e.state != TaskExecState::Success)
                            })
                            .copied()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                missing.sort();

                let preview = missing
                    .iter()
                    .take(5)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let reason = if missing.len() <= 5 {
                    format!("Waiting on dependencies: {preview}")
                } else {
                    format!(
                        "Waiting on dependencies: {preview} (+{} more)",
                        missing.len().saturating_sub(5)
                    )
                };

                if exec.blocked_reason.as_deref() != Some(reason.as_str()) {
                    newly_blocked.push((*task_id, reason));
                }
            }
        }

        for (task_id, reason) in newly_blocked {
            let event = Event::new(
                EventPayload::TaskBlocked {
                    flow_id: flow.id,
                    task_id,
                    reason: Some(reason),
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store
                .append(event)
                .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;
        }

        for task_id in newly_ready {
            let event = Event::new(
                EventPayload::TaskReady {
                    flow_id: flow.id,
                    task_id,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store
                .append(event)
                .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;
        }

        Ok(())
    }

    pub(super) fn select_task_to_run(
        state: &AppState,
        flow: &TaskFlow,
        preferred_task: Option<Uuid>,
    ) -> Option<Uuid> {
        let mut retrying = flow.tasks_in_state(TaskExecState::Retry);
        retrying.sort();
        let mut ready = flow.tasks_in_state(TaskExecState::Ready);
        ready.sort();

        let preferred = preferred_task.filter(|task_id| {
            (retrying.contains(task_id) || ready.contains(task_id))
                && Self::can_auto_run_task(state, *task_id)
        });
        preferred.or_else(|| {
            retrying
                .iter()
                .chain(ready.iter())
                .find(|task_id| Self::can_auto_run_task(state, **task_id))
                .copied()
        })
    }

    pub(super) fn maybe_complete_finished_flow(
        &self,
        flow: &TaskFlow,
        flow_id: &str,
        origin: &'static str,
    ) -> Result<Option<TaskFlow>> {
        let all_success = flow
            .task_executions
            .values()
            .all(|e| e.state == TaskExecState::Success);
        if !all_success {
            return Ok(None);
        }

        let event = Event::new(
            EventPayload::TaskFlowCompleted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;
        self.maybe_autostart_dependent_flows(flow.id)?;
        self.get_flow(flow_id).map(Some)
    }
}
