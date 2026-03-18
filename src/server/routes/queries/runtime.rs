use super::*;

pub(super) fn handle_runtime_queries(
    path: &str,
    url: &str,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let runtime_service = || app.runtime_service();

    let resp = match path {
        "/api/runtimes" => super::json_ok(runtime_service()?.runtime_list())?,
        "/api/runtimes/health" => {
            let query = parse_query(url);
            let role = parse_runtime_role(
                query.get("role").map(String::as_str),
                "server:runtimes:health",
            )?;
            super::json_ok(runtime_service()?.runtime_health_with_role(
                query.get("project").map(String::as_str),
                query.get("task").map(String::as_str),
                query.get("flow").map(String::as_str),
                role,
            )?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
