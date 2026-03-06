//! Worktree registry support.

#![allow(clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::shared_types::*;
use crate::core::registry::types::*;
use crate::core::registry::{
    Registry, ATTEMPT_CONTEXT_MAX_EXPAND_DEPTH, ATTEMPT_CONTEXT_SCHEMA_VERSION,
    ATTEMPT_CONTEXT_SECTION_BUDGET_BYTES, ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES,
    ATTEMPT_CONTEXT_TRUNCATION_POLICY, ATTEMPT_CONTEXT_VERSION, CONSTITUTION_SCHEMA_VERSION,
    CONSTITUTION_VERSION, GOVERNANCE_EXPORT_IMPORT_BOUNDARY, GOVERNANCE_FROM_LAYOUT,
    GOVERNANCE_PROJECTION_VERSION, GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION,
    GOVERNANCE_SCHEMA_VERSION, GOVERNANCE_TO_LAYOUT, GRAPH_SNAPSHOT_SCHEMA_VERSION,
    GRAPH_SNAPSHOT_VERSION,
};

mod artifacts;
mod git;
mod managers;
mod scope;

impl Registry {
    pub fn worktree_list(&self, flow_id: &str) -> Result<Vec<WorktreeStatus>> {
        let flow = self.get_flow(flow_id)?;
        let state = self.state()?;
        let managers = Self::worktree_managers_for_flow(&flow, &state, "registry:worktree_list")?;

        let mut statuses = Vec::new();
        for (_repo_name, manager) in managers {
            for task_id in flow.task_executions.keys() {
                let status = manager
                    .inspect(flow.id, *task_id)
                    .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_list"))?;
                statuses.push(status);
            }
        }
        Ok(statuses)
    }

    pub fn worktree_inspect(&self, task_id: &str) -> Result<WorktreeStatus> {
        let tid = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:worktree_inspect",
            )
        })?;

        let state = self.state()?;
        let mut candidates: Vec<&TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&tid))
            .collect();

        if candidates.is_empty() {
            return Err(HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:worktree_inspect",
            ));
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let manager = Self::worktree_manager_for_flow(&flow, &state)?;
        manager
            .inspect(flow.id, tid)
            .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_inspect"))
    }

    pub fn worktree_cleanup(
        &self,
        flow_id: &str,
        force: bool,
        dry_run: bool,
    ) -> Result<WorktreeCleanupResult> {
        let flow = self.get_flow(flow_id)?;
        if flow.state == FlowState::Running && !force {
            return Err(HivemindError::user(
                "flow_running_cleanup_requires_force",
                "Flow is running; pass --force to clean up active worktrees",
                "registry:worktree_cleanup",
            ));
        }

        let existing_worktrees = self
            .worktree_list(flow_id)?
            .iter()
            .filter(|status| status.is_worktree)
            .count();

        let state = self.state()?;
        let managers =
            Self::worktree_managers_for_flow(&flow, &state, "registry:worktree_cleanup")?;
        if !dry_run {
            for (_repo_name, manager) in managers {
                manager.cleanup_flow(flow.id).map_err(|e| {
                    Self::worktree_error_to_hivemind(e, "registry:worktree_cleanup")
                })?;
            }
        }

        self.append_event(
            Event::new(
                EventPayload::WorktreeCleanupPerformed {
                    flow_id: flow.id,
                    cleaned_worktrees: existing_worktrees,
                    forced: force,
                    dry_run,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:worktree_cleanup",
        )?;

        Ok(WorktreeCleanupResult {
            flow_id: flow.id,
            cleaned_worktrees: existing_worktrees,
            forced: force,
            dry_run,
        })
    }
}
