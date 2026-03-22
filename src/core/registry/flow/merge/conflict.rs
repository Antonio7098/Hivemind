use super::*;

impl Registry {
    pub(crate) fn emit_merge_conflict(
        &self,
        flow: &TaskFlow,
        task_id: Option<Uuid>,
        details: String,
        origin: &'static str,
    ) -> Result<()> {
        let state = self.state()?;
        self.append_event(
            Event::new(
                EventPayload::MergeConflictDetected {
                    flow_id: flow.id,
                    task_id,
                    details,
                },
                task_id.map_or_else(
                    || Self::correlation_for_flow_event(&state, flow),
                    |task_id| Self::correlation_for_flow_task_event(&state, flow, task_id),
                ),
            ),
            origin,
        )
    }
}
