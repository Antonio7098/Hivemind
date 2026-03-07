use super::*;
mod checks;
mod common;
mod primary;
mod secondary;

impl Registry {
    pub fn merge_prepare(
        &self,
        flow_id: &str,
        target_branch: Option<&str>,
    ) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_prepare";
        let mut flow = self.get_flow(flow_id)?;

        if !matches!(flow.state, FlowState::Completed | FlowState::FrozenForMerge) {
            let err = HivemindError::user(
                "flow_not_completed",
                "Flow has not completed successfully",
                origin,
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            );
            return Err(err);
        }

        let _ = self.enforce_constitution_gate(
            flow.project_id,
            "merge_prepare",
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            origin,
        )?;

        let mut state = self.state()?;
        if let Some(ms) = state.merge_states.get(&flow.id) {
            if ms.status == crate::core::state::MergeStatus::Prepared && ms.conflicts.is_empty() {
                return Ok(ms.clone());
            }
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_prepare", origin)?;

        if flow.state == FlowState::Completed {
            self.append_event(
                Event::new(
                    EventPayload::FlowFrozenForMerge { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;
            flow = self.get_flow(flow_id)?;
            state = self.state()?;
        }

        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;
        let successful_task_ids = Self::successful_task_ids_in_topological_order(&flow, graph);

        let mut managers = Self::worktree_managers_for_flow(&flow, &state, origin)?;
        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system("project_not_found", "Project not found", origin)
        })?;
        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                origin,
            ));
        }
        let (_primary_repo_name, manager) = managers.drain(..1).next().ok_or_else(|| {
            HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                origin,
            )
        })?;

        let prepared_target_branch =
            Self::resolve_prepare_target_branch(&manager, target_branch, origin)?;
        let primary = self.prepare_primary_repository(
            &flow,
            graph,
            &successful_task_ids,
            &manager,
            &prepared_target_branch,
            origin,
        )?;
        let mut conflicts = primary.conflicts;

        if conflicts.is_empty() {
            conflicts.extend(self.run_aggregated_merge_checks(
                &flow,
                graph,
                &successful_task_ids,
                &primary.merge_path,
                origin,
            )?);
        }

        if conflicts.is_empty() {
            self.publish_primary_prepare_branch(&manager, flow.id, &primary.merge_branch, origin)?;
            for (task_id, commit_sha) in &primary.integrated_tasks {
                self.append_event(
                    Event::new(
                        EventPayload::TaskIntegratedIntoFlow {
                            flow_id: flow.id,
                            task_id: *task_id,
                            commit_sha: commit_sha.clone(),
                        },
                        CorrelationIds::for_graph_flow_task(
                            flow.project_id,
                            flow.graph_id,
                            flow.id,
                            *task_id,
                        ),
                    ),
                    origin,
                )?;
            }
        }

        if conflicts.is_empty() {
            conflicts.extend(self.prepare_secondary_repositories(
                &flow,
                &successful_task_ids,
                managers,
                &prepared_target_branch,
                origin,
            )?);
        }

        let event = Event::new(
            EventPayload::MergePrepared {
                flow_id: flow.id,
                target_branch: Some(prepared_target_branch),
                conflicts,
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
                "Merge state not found after prepare",
                origin,
            )
        })
    }
}
