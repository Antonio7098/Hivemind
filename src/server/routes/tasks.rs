use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/tasks/create" => {
            let req: TaskCreateRequest = parse_json_body(body, "server:tasks:create")?;
            super::json_ok(registry.create_task(
                &req.project,
                &req.title,
                req.description.as_deref(),
                req.scope,
            )?)?
        }
        "/api/tasks/update" => {
            let req: TaskUpdateRequest = parse_json_body(body, "server:tasks:update")?;
            super::json_ok(registry.update_task(
                &req.task_id,
                req.title.as_deref(),
                req.description.as_deref(),
            )?)?
        }
        "/api/tasks/delete" => {
            let req: TaskDeleteRequest = parse_json_body(body, "server:tasks:delete")?;
            super::json_ok(serde_json::json!({
                "task_id": registry.delete_task(&req.task_id)?,
            }))?
        }
        "/api/tasks/close" => {
            let req: TaskCloseRequest = parse_json_body(body, "server:tasks:close")?;
            super::json_ok(registry.close_task(&req.task_id, req.reason.as_deref())?)?
        }
        "/api/tasks/start" => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:start")?;
            super::json_ok(serde_json::json!({
                "attempt_id": registry.start_task_execution(&req.task_id)?,
            }))?
        }
        "/api/tasks/complete" => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:complete")?;
            super::json_ok(registry.complete_task_execution(&req.task_id)?)?
        }
        "/api/tasks/retry" => {
            let req: TaskRetryRequest = parse_json_body(body, "server:tasks:retry")?;
            super::json_ok(registry.retry_task(
                &req.task_id,
                req.reset_count.unwrap_or(false),
                parse_retry_mode(req.mode.as_deref())?,
            )?)?
        }
        "/api/tasks/abort" => {
            let req: TaskAbortRequest = parse_json_body(body, "server:tasks:abort")?;
            super::json_ok(registry.abort_task(&req.task_id, req.reason.as_deref())?)?
        }
        "/api/tasks/runtime" => {
            let req: TaskRuntimeSetRequest = parse_json_body(body, "server:tasks:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:tasks:runtime")?;
            if req.clear.unwrap_or(false) {
                super::json_ok(registry.task_runtime_clear_role(&req.task_id, role)?)?
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                super::json_ok(registry.task_runtime_set_role(
                    &req.task_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                )?)?
            }
        }
        "/api/tasks/run-mode" => {
            let req: TaskSetRunModeRequest = parse_json_body(body, "server:tasks:run-mode")?;
            super::json_ok(registry.task_set_run_mode(
                &req.task_id,
                parse_run_mode(&req.mode, "server:tasks:run-mode")?,
            )?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

fn parse_retry_mode(mode: Option<&str>) -> Result<RetryMode> {
    match mode.unwrap_or("clean").to_lowercase().as_str() {
        "clean" => Ok(RetryMode::Clean),
        "continue" => Ok(RetryMode::Continue),
        other => Err(HivemindError::user(
            "invalid_retry_mode",
            format!("Invalid retry mode '{other}'"),
            "server:tasks:retry",
        )),
    }
}
