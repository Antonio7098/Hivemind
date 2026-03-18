use super::*;

pub(super) fn handle_state_queries(
    path: &str,
    url: &str,
    default_events_limit: usize,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let state_service = || app.state_service();
    let project_service = || app.project_service();
    let event_service = || app.event_service();

    let resp = match path {
        "/api/state" => {
            let query = parse_query(url);
            let events_limit = query
                .get("events_limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(build_ui_state(
                &state_service()?,
                &event_service()?,
                events_limit,
            )?)?
        }
        "/api/projects" => super::json_ok(project_service()?.list_projects()?)?,
        "/api/tasks" => super::json_ok(list_tasks(&state_service()?)?)?,
        "/api/graphs" => super::json_ok(list_graphs(&state_service()?)?)?,
        "/api/flows" => super::json_ok(list_flows(&state_service()?)?)?,
        "/api/merges" => super::json_ok(list_merge_states(&state_service()?)?)?,
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
