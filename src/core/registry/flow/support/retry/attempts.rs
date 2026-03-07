use super::*;

impl Registry {
    pub(crate) fn resolve_latest_attempt_without_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_none())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for running task",
                    origin,
                )
            })
    }

    pub(crate) fn fail_running_attempt(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        reason: &str,
        origin: &'static str,
    ) -> Result<()> {
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    from: TaskExecState::Running,
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
                    attempt_id: Some(attempt_id),
                    reason: Some(reason.to_string()),
                },
                corr_attempt,
            ),
            origin,
        )
    }

    pub(crate) fn resolve_latest_attempt_with_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_some())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for verifying task",
                    origin,
                )
            })
    }
}
