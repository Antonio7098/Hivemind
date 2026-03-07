use super::*;

impl Registry {
    pub(crate) fn unmet_flow_dependencies(state: &AppState, flow: &TaskFlow) -> Vec<Uuid> {
        let mut unmet: Vec<Uuid> = flow
            .depends_on_flows
            .iter()
            .filter(|dep_id| {
                state.flows.get(dep_id).is_none_or(|dep| {
                    !matches!(dep.state, FlowState::Completed | FlowState::Merged)
                })
            })
            .copied()
            .collect();
        unmet.sort();
        unmet
    }

    pub(crate) fn can_auto_run_task(state: &AppState, task_id: Uuid) -> bool {
        state
            .tasks
            .get(&task_id)
            .is_none_or(|task| task.run_mode == RunMode::Auto)
    }

    pub(crate) fn maybe_autostart_dependent_flows(&self, completed_flow_id: Uuid) -> Result<()> {
        let state = self.state()?;
        let mut candidates: Vec<Uuid> = state
            .flows
            .values()
            .filter(|flow| {
                flow.state == FlowState::Created
                    && flow.run_mode == RunMode::Auto
                    && flow.depends_on_flows.contains(&completed_flow_id)
                    && Self::unmet_flow_dependencies(&state, flow).is_empty()
            })
            .map(|flow| flow.id)
            .collect();
        candidates.sort();

        for candidate in candidates {
            let _ = self.start_flow(&candidate.to_string())?;
        }
        Ok(())
    }
}
