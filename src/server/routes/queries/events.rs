use super::*;

fn build_event_filter(
    query: &std::collections::HashMap<String, String>,
    default_events_limit: usize,
) -> Result<EventFilter> {
    let mut filter = EventFilter::all();

    if let Some(v) = query.get("limit") {
        filter.limit = v.parse::<usize>().ok();
    } else {
        filter.limit = Some(default_events_limit);
    }

    if let Some(v) = query.get("offset") {
        filter.offset = v.parse::<usize>().ok();
    }

    if let Some(v) = query.get("project_id") {
        filter.project_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_project_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("graph_id") {
        filter.graph_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_graph_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("task_id") {
        filter.task_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("flow_id") {
        filter.flow_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("workflow_id") {
        filter.workflow_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_workflow_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("workflow_run_id") {
        filter.workflow_run_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_workflow_run_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("step_id") {
        filter.step_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_step_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("step_run_id") {
        filter.step_run_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_step_run_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("attempt_id") {
        filter.attempt_id = Some(Uuid::parse_str(v).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("Invalid UUID: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("since") {
        filter.since = Some(v.parse::<DateTime<Utc>>().map_err(|_| {
            HivemindError::user(
                "invalid_since",
                format!("Invalid datetime: {v}"),
                "server:events",
            )
        })?);
    }
    if let Some(v) = query.get("until") {
        filter.until = Some(v.parse::<DateTime<Utc>>().map_err(|_| {
            HivemindError::user(
                "invalid_until",
                format!("Invalid datetime: {v}"),
                "server:events",
            )
        })?);
    }

    Ok(filter)
}

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
            let filter = build_event_filter(&query, default_events_limit)?;
            super::json_ok(list_ui_events_filtered(&event_service()?, &filter)?)?
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
