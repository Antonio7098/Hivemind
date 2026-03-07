use super::*;

impl Registry {
    pub fn merge_approve(&self, flow_id: &str) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;
        let origin = "registry:merge_approve";

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "merge_approve",
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            origin,
        )?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                origin,
            )
        })?;

        if ms.status == crate::core::state::MergeStatus::Approved {
            return Ok(ms.clone());
        }

        if !ms.conflicts.is_empty() {
            return Err(HivemindError::user(
                "unresolved_conflicts",
                "Merge has unresolved conflicts",
                origin,
            ));
        }

        let user = env::var("HIVEMIND_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty());

        let event = Event::new(
            EventPayload::MergeApproved {
                flow_id: flow.id,
                user,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after approve",
                origin,
            )
        })
    }
}
