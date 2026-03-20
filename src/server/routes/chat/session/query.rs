use super::super::scope::{get_chat_session, resolve_chat_scope};
use super::super::view::{map_session_inspect, map_session_summary};
use crate::app::ChatService;
use crate::core::error::{HivemindError, Result};
use crate::server::api_types::{ChatSessionInspectView, ChatSessionSummaryView};
use crate::server::routes::parse_query;

pub(crate) fn list_chat_sessions(
    url: &str,
    service: &ChatService,
) -> Result<Vec<ChatSessionSummaryView>> {
    let query = parse_query(url);
    let scope = resolve_chat_scope(
        service,
        query.get("project").map(String::as_str),
        query.get("task").map(String::as_str),
        query.get("flow").map(String::as_str),
        "server:chat:list",
    )?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50);

    let state = service.state()?;
    let mut sessions = state.chat_sessions.values().cloned().collect::<Vec<_>>();
    sessions.retain(|session| {
        scope
            .project_id
            .is_none_or(|value| session.project_id == Some(value))
            && scope
                .task_id
                .is_none_or(|value| session.task_id == Some(value))
            && scope
                .flow_id
                .is_none_or(|value| session.flow_id == Some(value))
    });
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    sessions.truncate(limit);

    Ok(sessions.iter().map(map_session_summary).collect())
}

pub(crate) fn inspect_chat_session(
    url: &str,
    service: &ChatService,
) -> Result<ChatSessionInspectView> {
    let query = parse_query(url);
    let session_id = query.get("session_id").ok_or_else(|| {
        HivemindError::user(
            "chat_session_id_required",
            "session_id is required",
            "server:chat:inspect",
        )
    })?;
    let session = get_chat_session(service, session_id, "server:chat:inspect")?;
    Ok(map_session_inspect(&session))
}
