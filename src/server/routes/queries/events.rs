use super::*;

pub(super) fn handle_event_queries(
    path: &str,
    url: &str,
    default_events_limit: usize,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let event_service = || app.event_service();

    let resp = match path {
        "/api/events" => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(list_ui_events(&event_service()?, limit)?)?
        }
        "/api/events/inspect" => {
            let query = parse_query(url);
            let event_id = query.get("event_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_event_id",
                    "Query parameter 'event_id' is required",
                    "server:events:inspect",
                )
            })?;
            super::json_ok(event_service()?.get_event(event_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
