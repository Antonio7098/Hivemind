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
            super::json_ok(registry.merge_prepare(&req.flow_id, req.target.as_deref())?)?
        }
        "/api/merge/approve" => {
            let req: MergeApproveRequest = parse_json_body(body, "server:merge:approve")?;
            super::json_ok(registry.merge_approve(&req.flow_id)?)?
        }
        "/api/merge/execute" => {
            let req: MergeExecuteRequest = parse_json_body(body, "server:merge:execute")?;
            super::json_ok(registry.merge_execute_with_options(
                &req.flow_id,
                MergeExecuteOptions {
                    mode: parse_merge_mode(req.mode.as_deref(), "server:merge:execute")?,
                    monitor_ci: req.monitor_ci.unwrap_or(false),
                    auto_merge: req.auto_merge.unwrap_or(false),
                    pull_after: req.pull_after.unwrap_or(false),
                },
            )?)?
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
            super::json_ok(registry.worktree_cleanup(&req.flow_id, req.force, req.dry_run)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
