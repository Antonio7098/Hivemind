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
    fn with_workflow_lineage(
        status: &mut WorktreeStatus,
        run: &WorkflowRun,
        workflow: &WorkflowDefinition,
    ) {
        status.workflow_id = Some(workflow.id);
        status.workflow_run_id = Some(run.id);
        if let Some((step_id, step_run_id)) =
            Self::workflow_bridge_step_for_task(run, status.task_id)
        {
            status.step_id = Some(step_id);
            status.step_run_id = Some(step_run_id);
        }
    }

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

    pub fn workflow_worktree_list(&self, workflow_run_id: &str) -> Result<Vec<WorktreeStatus>> {
        let (run, workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let Some(flow) = flow else {
            return Ok(Vec::new());
        };
        let mut statuses = self.worktree_list(&flow.id.to_string())?;
        for status in &mut statuses {
            Self::with_workflow_lineage(status, &run, &workflow);
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

    pub fn workflow_worktree_inspect(
        &self,
        workflow_run_id: &str,
        step_id: &str,
    ) -> Result<WorktreeStatus> {
        let (run, workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let Some(flow) = flow else {
            return Err(HivemindError::user(
                "workflow_run_has_no_execution_bridge",
                "Workflow run has not created execution worktrees yet",
                "registry:workflow_worktree_inspect",
            ));
        };
        let step_id = Uuid::parse_str(step_id).map_err(|_| {
            HivemindError::user(
                "invalid_step_id",
                format!("'{step_id}' is not a valid workflow step ID"),
                "registry:workflow_worktree_inspect",
            )
        })?;
        let step = workflow.steps.get(&step_id).ok_or_else(|| {
            HivemindError::user(
                "workflow_step_not_found",
                format!("Workflow step '{step_id}' not found"),
                "registry:workflow_worktree_inspect",
            )
        })?;
        let task_id = Self::workflow_bridge_task_id(run.id, step.id);
        let state = self.state()?;
        let manager = Self::worktree_manager_for_flow(&flow, &state)?;
        let mut status = manager.inspect(flow.id, task_id).map_err(|e| {
            Self::worktree_error_to_hivemind(e, "registry:workflow_worktree_inspect")
        })?;
        Self::with_workflow_lineage(&mut status, &run, &workflow);
        Ok(status)
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

    pub fn workflow_worktree_cleanup(
        &self,
        workflow_run_id: &str,
        force: bool,
        dry_run: bool,
    ) -> Result<WorktreeCleanupResult> {
        let (run, _workflow, flow) = self.workflow_run_bridge_flow(workflow_run_id)?;
        let flow_id = flow
            .map(|flow| flow.id)
            .unwrap_or_else(|| Self::workflow_bridge_flow_uuid(run.id));
        self.worktree_cleanup(&flow_id.to_string(), force, dry_run)
    }

    pub fn worktree_restore_turn_ref(
        &self,
        attempt_id: &str,
        ordinal: u32,
        confirm: bool,
        force: bool,
    ) -> Result<WorktreeTurnRestoreResult> {
        if !confirm {
            return Err(HivemindError::user(
                "confirmation_required",
                "Restoring a turn ref discards current worktree changes; pass --confirm to proceed",
                "registry:worktree_restore_turn_ref",
            ));
        }

        let attempt = self.get_attempt(attempt_id)?;
        let turn_ref = attempt
            .turn_refs
            .iter()
            .find(|turn| turn.ordinal == ordinal)
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "turn_ref_not_found",
                    format!("Attempt '{attempt_id}' does not have a stored turn ref for ordinal {ordinal}"),
                    "registry:worktree_restore_turn_ref",
                )
            })?;

        let flow = self.get_flow(&attempt.flow_id.to_string())?;
        if flow.state == FlowState::Running && !force {
            return Err(HivemindError::user(
                "flow_running_restore_requires_force",
                "Flow is running; pass --force to restore an active worktree turn ref",
                "registry:worktree_restore_turn_ref",
            ));
        }

        let reference = turn_ref
            .git_ref
            .clone()
            .or_else(|| turn_ref.commit_sha.clone())
            .ok_or_else(|| {
                HivemindError::user(
                    "turn_ref_missing_reference",
                    format!("Attempt '{attempt_id}' turn {ordinal} has no git ref or commit SHA"),
                    "registry:worktree_restore_turn_ref",
                )
            })?;

        let state = self.state()?;
        let manager = Self::worktree_manager_for_flow(&flow, &state)?;
        let status = manager.inspect(flow.id, attempt.task_id).map_err(|e| {
            Self::worktree_error_to_hivemind(e, "registry:worktree_restore_turn_ref")
        })?;
        if !status.is_worktree {
            return Err(HivemindError::user(
                "worktree_not_found",
                format!("No active worktree exists for task '{}'", attempt.task_id),
                "registry:worktree_restore_turn_ref",
            ));
        }

        let had_local_changes = manager.has_uncommitted_changes(&status.path).map_err(|e| {
            Self::worktree_error_to_hivemind(e, "registry:worktree_restore_turn_ref")
        })?;
        let head_before = manager.worktree_head(&status.path).map_err(|e| {
            Self::worktree_error_to_hivemind(e, "registry:worktree_restore_turn_ref")
        })?;
        manager
            .restore_hidden_snapshot_ref(&status.path, &reference)
            .map_err(|e| {
                Self::worktree_error_to_hivemind(e, "registry:worktree_restore_turn_ref")
            })?;
        let head_after = manager.worktree_head(&status.path).map_err(|e| {
            Self::worktree_error_to_hivemind(e, "registry:worktree_restore_turn_ref")
        })?;

        self.append_event(
            Event::new(
                EventPayload::WorktreeTurnRefRestored {
                    flow_id: flow.id,
                    task_id: attempt.task_id,
                    attempt_id: attempt.id,
                    ordinal,
                    git_ref: turn_ref.git_ref.clone(),
                    commit_sha: turn_ref.commit_sha.clone(),
                    forced: force,
                },
                CorrelationIds::for_graph_flow_task_attempt(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    attempt.task_id,
                    attempt.id,
                ),
            ),
            "registry:worktree_restore_turn_ref",
        )?;

        Ok(WorktreeTurnRestoreResult {
            flow_id: flow.id,
            task_id: attempt.task_id,
            attempt_id: attempt.id,
            ordinal,
            git_ref: turn_ref.git_ref,
            commit_sha: turn_ref.commit_sha,
            worktree_path: status.path,
            branch: status.branch,
            head_before,
            head_after,
            had_local_changes,
            forced: force,
        })
    }
}
