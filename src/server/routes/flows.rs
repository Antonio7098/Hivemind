use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/flows/create" => {
            let req: FlowCreateRequest = parse_json_body(body, "server:flows:create")?;
            super::json_ok(registry.create_flow(&req.graph_id, req.name.as_deref())?)?
        }
        "/api/flows/delete" => {
            let req: FlowDeleteRequest = parse_json_body(body, "server:flows:delete")?;
            super::json_ok(serde_json::json!({
                "flow_id": registry.delete_flow(&req.flow_id)?,
            }))?
        }
        "/api/flows/start" => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:start")?;
            super::json_ok(registry.start_flow(&req.flow_id)?)?
        }
        "/api/flows/tick" => {
            let req: FlowTickRequest = parse_json_body(body, "server:flows:tick")?;
            super::json_ok(registry.tick_flow(
                &req.flow_id,
                req.interactive.unwrap_or(false),
                req.max_parallel,
            )?)?
        }
        "/api/flows/pause" => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:pause")?;
            super::json_ok(registry.pause_flow(&req.flow_id)?)?
        }
        "/api/flows/resume" => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:resume")?;
            super::json_ok(registry.resume_flow(&req.flow_id)?)?
        }
        "/api/flows/abort" => {
            let req: FlowAbortRequest = parse_json_body(body, "server:flows:abort")?;
            super::json_ok(registry.abort_flow(
                &req.flow_id,
                req.reason.as_deref(),
                req.force.unwrap_or(false),
            )?)?
        }
        "/api/flows/run-mode" => {
            let req: FlowSetRunModeRequest = parse_json_body(body, "server:flows:run-mode")?;
            super::json_ok(registry.flow_set_run_mode(
                &req.flow_id,
                parse_run_mode(&req.mode, "server:flows:run-mode")?,
            )?)?
        }
        "/api/flows/dependencies/add" => {
            let req: FlowAddDependencyRequest =
                parse_json_body(body, "server:flows:dependencies:add")?;
            super::json_ok(registry.flow_add_dependency(&req.flow_id, &req.depends_on_flow_id)?)?
        }
        "/api/flows/runtime" => {
            let req: FlowRuntimeSetRequest = parse_json_body(body, "server:flows:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:flows:runtime")?;
            if req.clear.unwrap_or(false) {
                super::json_ok(registry.flow_runtime_clear(&req.flow_id, role)?)?
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                super::json_ok(registry.flow_runtime_set(
                    &req.flow_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                    req.max_parallel_tasks.unwrap_or(1),
                )?)?
            }
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
