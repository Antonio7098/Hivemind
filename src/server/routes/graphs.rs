use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/graphs/create" => {
            let req: GraphCreateRequest = parse_json_body(body, "server:graphs:create")?;
            let from_tasks = req
                .from_tasks
                .iter()
                .map(|s| {
                    uuid::Uuid::parse_str(s).map_err(|_| {
                        HivemindError::user(
                            "invalid_task_id",
                            format!("'{s}' is not a valid task ID"),
                            "server:graphs:create",
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            super::json_ok(registry.create_graph(&req.project, &req.name, &from_tasks)?)?
        }
        "/api/graphs/delete" => {
            let req: GraphDeleteRequest = parse_json_body(body, "server:graphs:delete")?;
            super::json_ok(serde_json::json!({
                "graph_id": registry.delete_graph(&req.graph_id)?,
            }))?
        }
        "/api/graphs/dependencies/add" => {
            let req: GraphDependencyRequest =
                parse_json_body(body, "server:graphs:dependencies:add")?;
            super::json_ok(registry.add_graph_dependency(
                &req.graph_id,
                &req.from_task,
                &req.to_task,
            )?)?
        }
        "/api/graphs/checks/add" => {
            let req: GraphAddCheckRequest = parse_json_body(body, "server:graphs:checks:add")?;
            let mut check = CheckConfig::new(req.name, req.command);
            check.required = req.required.unwrap_or(true);
            check.timeout_ms = req.timeout_ms;
            super::json_ok(registry.add_graph_task_check(&req.graph_id, &req.task_id, check)?)?
        }
        "/api/graphs/validate" => {
            let req: GraphValidateRequest = parse_json_body(body, "server:graphs:validate")?;
            super::json_ok(registry.validate_graph(&req.graph_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
