use super::*;

pub(super) mod attempts;
pub(super) mod events;
pub(super) mod other;
pub(super) mod runtime;
pub(super) mod state;

pub(super) fn handle_get(
    path: &str,
    url: &str,
    default_events_limit: usize,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    if let Some(resp) = state::handle_state_queries(path, url, default_events_limit, app)? {
        return Ok(Some(resp));
    }

    if let Some(resp) = runtime::handle_runtime_queries(path, url, app)? {
        return Ok(Some(resp));
    }

    if let Some(resp) = events::handle_event_queries(path, url, default_events_limit, app)? {
        return Ok(Some(resp));
    }

    if let Some(resp) = attempts::handle_attempt_queries(path, url, app)? {
        return Ok(Some(resp));
    }

    if let Some(resp) = other::handle_other_queries(path, url, app)? {
        return Ok(Some(resp));
    }

    let resp = match path {
        "/health" => {
            let mut resp = ApiResponse::text(200, "text/plain", "ok\n");
            resp.extra_headers.extend(cors_headers());
            resp
        }
        "/api/version" => {
            super::json_ok(serde_json::json!({"version": env!("CARGO_PKG_VERSION")}))?
        }
        "/api/catalog" => super::json_ok(api_catalog())?,
        "/api/runtime-stream" => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(app.runtime_service()?.runtime_stream_items_with_detail(
                query.get("flow_id").map(String::as_str),
                query.get("attempt_id").map(String::as_str),
                limit,
                parse_runtime_stream_detail(&query)?,
            )?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
