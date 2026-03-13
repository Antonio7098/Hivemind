use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/verify/override" => {
            let req: VerifyOverrideRequest = parse_json_body(body, "server:verify:override")?;
            super::json_ok(registry.verify_override(&req.task_id, &req.decision, &req.reason)?)?
        }
        "/api/verify/run" => {
            let req: VerifyRunRequest = parse_json_body(body, "server:verify:run")?;
            super::json_ok(registry.verify_run(&req.task_id)?)?
        }
        "/api/merge/prepare" => {
            let req: MergePrepareRequest = parse_json_body(body, "server:merge:prepare")?;
            super::json_ok(
                match (req.workflow_run_id.as_deref(), req.flow_id.as_deref()) {
                    (Some(workflow_run_id), _) => {
                        registry.workflow_merge_prepare(workflow_run_id, req.target.as_deref())?
                    }
                    (None, Some(flow_id)) => {
                        registry.merge_prepare(flow_id, req.target.as_deref())?
                    }
                    (None, None) => {
                        return Err(HivemindError::user(
                            "missing_merge_owner",
                            "Either 'workflow_run_id' or 'flow_id' is required",
                            "server:merge:prepare",
                        ))
                    }
                },
            )?
        }
        "/api/merge/approve" => {
            let req: MergeApproveRequest = parse_json_body(body, "server:merge:approve")?;
            super::json_ok(
                match (req.workflow_run_id.as_deref(), req.flow_id.as_deref()) {
                    (Some(workflow_run_id), _) => {
                        registry.workflow_merge_approve(workflow_run_id)?
                    }
                    (None, Some(flow_id)) => registry.merge_approve(flow_id)?,
                    (None, None) => {
                        return Err(HivemindError::user(
                            "missing_merge_owner",
                            "Either 'workflow_run_id' or 'flow_id' is required",
                            "server:merge:approve",
                        ))
                    }
                },
            )?
        }
        "/api/merge/execute" => {
            let req: MergeExecuteRequest = parse_json_body(body, "server:merge:execute")?;
            let options = MergeExecuteOptions {
                mode: parse_merge_mode(req.mode.as_deref(), "server:merge:execute")?,
                monitor_ci: req.monitor_ci.unwrap_or(false),
                auto_merge: req.auto_merge.unwrap_or(false),
                pull_after: req.pull_after.unwrap_or(false),
            };
            super::json_ok(
                match (req.workflow_run_id.as_deref(), req.flow_id.as_deref()) {
                    (Some(workflow_run_id), _) => {
                        registry.workflow_merge_execute_with_options(workflow_run_id, options)?
                    }
                    (None, Some(flow_id)) => {
                        registry.merge_execute_with_options(flow_id, options)?
                    }
                    (None, None) => {
                        return Err(HivemindError::user(
                            "missing_merge_owner",
                            "Either 'workflow_run_id' or 'flow_id' is required",
                            "server:merge:execute",
                        ))
                    }
                },
            )?
        }
        "/api/checkpoints/complete" => {
            let req: CheckpointCompleteRequest =
                parse_json_body(body, "server:checkpoints:complete")?;
            super::json_ok(registry.checkpoint_complete(
                &req.attempt_id,
                &req.checkpoint_id,
                req.summary.as_deref(),
            )?)?
        }
        "/api/worktrees/cleanup" => {
            let req: WorktreeCleanupRequest = parse_json_body(body, "server:worktrees:cleanup")?;
            super::json_ok(
                match (req.workflow_run_id.as_deref(), req.flow_id.as_deref()) {
                    (Some(workflow_run_id), _) => registry.workflow_worktree_cleanup(
                        workflow_run_id,
                        req.force,
                        req.dry_run,
                    )?,
                    (None, Some(flow_id)) => {
                        registry.worktree_cleanup(flow_id, req.force, req.dry_run)?
                    }
                    (None, None) => {
                        return Err(HivemindError::user(
                            "missing_worktree_owner",
                            "Either 'workflow_run_id' or 'flow_id' is required",
                            "server:worktrees:cleanup",
                        ))
                    }
                },
            )?
        }
        "/api/worktrees/restore-turn" => {
            let req: WorktreeRestoreTurnRequest =
                parse_json_body(body, "server:worktrees:restore-turn")?;
            super::json_ok(registry.worktree_restore_turn_ref(
                &req.attempt_id,
                req.ordinal,
                req.confirm,
                req.force,
            )?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
