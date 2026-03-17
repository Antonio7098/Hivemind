use super::*;
use crate::app::ChatService;
use crate::core::events::RuntimeSelectionSource;

mod execution;
mod scope;
mod view;
use crate::core::scope::RepoAccessMode;
use crate::core::state::{ChatMessageState, ChatSessionState, ProjectRuntimeConfig};
use crate::native::adapter::NativeAdapterConfig;
use crate::native::{
    AgentLoop, AgentLoopResult, AgentLoopState, AgentLoopTurn, MockModelClient, ModelClient,
    ModelDirective, OpenRouterModelClient,
};
use chrono::Utc;
use execution::{
    agent_state_label, directive_view, execute_chat, validate_chat_message, ChatExecutionRequest,
};
use scope::{
    chat_correlation, default_session_title, get_chat_session, parse_mode, resolve_chat_scope,
    resolve_chat_scope_from_session, session_history_inputs, ResolvedChatScope,
};
use std::time::Duration;
use view::{map_session_inspect, map_session_summary, preview_text};

const DEFAULT_CHAT_MODEL: &str = "openrouter/meta-llama/llama-3.2-3b-instruct:free";
const DEFAULT_CHAT_TIMEOUT_MS: u64 = 60_000;

pub(super) fn handle_get(path: &str, url: &str, app: &AppContext) -> Result<Option<ApiResponse>> {
    let service = app.chat_service()?;
    let resp = match path {
        "/api/chat/sessions" => super::json_ok(list_chat_sessions(url, &service)?)?,
        "/api/chat/sessions/inspect" => super::json_ok(inspect_chat_session(url, &service)?)?,
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let service = app.chat_service()?;
    let resp = match path {
        "/api/chat/invoke" => {
            let req: ChatInvokeRequest = parse_json_body(body, "server:chat:invoke")?;
            super::json_ok(invoke_chat(&service, &req)?)?
        }
        "/api/chat/sessions/create" => {
            let req: ChatSessionCreateRequest = parse_json_body(body, "server:chat:create")?;
            super::json_ok(create_chat_session(&service, &req)?)?
        }
        "/api/chat/sessions/send" => {
            let req: ChatSessionSendRequest = parse_json_body(body, "server:chat:send")?;
            super::json_ok(send_chat_session_message(&service, &req)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

pub(crate) fn stream_envelope(
    event: &Event,
    session_filter: Option<Uuid>,
) -> Option<ChatStreamEnvelope> {
    match &event.payload {
        EventPayload::ChatMessageAppended {
            session_id,
            message_id,
            role,
            content,
            request_id,
            provider,
            model,
            final_state,
            runtime_selection_source,
        } => {
            if session_filter.is_some() && session_filter != Some(*session_id) {
                return None;
            }

            Some(ChatStreamEnvelope {
                cursor: event.metadata.sequence.unwrap_or_default(),
                session_id: session_id.to_string(),
                event: ChatStreamEvent::MessageAppended {
                    message: ChatSessionMessageView {
                        message_id: message_id.to_string(),
                        role: role.clone(),
                        content: content.clone(),
                        created_at: event.timestamp().to_rfc3339(),
                        request_id: request_id.clone(),
                        provider: provider.clone(),
                        model: model.clone(),
                        final_state: final_state.clone(),
                        runtime_selection_source: runtime_selection_source.clone(),
                    },
                },
            })
        }
        EventPayload::ChatStreamChunkAppended {
            session_id,
            message_id,
            request_id,
            turn_index,
            from_state,
            to_state,
            directive_kind,
            content,
        } => {
            if session_filter.is_some() && session_filter != Some(*session_id) {
                return None;
            }

            Some(ChatStreamEnvelope {
                cursor: event.metadata.sequence.unwrap_or_default(),
                session_id: session_id.to_string(),
                event: ChatStreamEvent::StreamChunk {
                    chunk: ChatStreamChunkView {
                        session_id: session_id.to_string(),
                        message_id: message_id.to_string(),
                        request_id: request_id.clone(),
                        turn_index: *turn_index,
                        from_state: from_state.clone(),
                        to_state: to_state.clone(),
                        directive_kind: directive_kind.clone(),
                        content: content.clone(),
                    },
                },
            })
        }
        _ => None,
    }
}

fn list_chat_sessions(url: &str, service: &ChatService) -> Result<Vec<ChatSessionSummaryView>> {
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

fn inspect_chat_session(url: &str, service: &ChatService) -> Result<ChatSessionInspectView> {
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

fn create_chat_session(
    service: &ChatService,
    req: &ChatSessionCreateRequest,
) -> Result<ChatSessionInspectView> {
    let scope = resolve_chat_scope(
        service,
        req.project.as_deref(),
        req.task.as_deref(),
        req.flow.as_deref(),
        "server:chat:create",
    )?;
    let session_id = Uuid::new_v4();
    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(|| default_session_title(req.mode), ToString::to_string);
    let correlation = chat_correlation(&scope);
    service.append_event(
        Event::new(
            EventPayload::ChatSessionCreated {
                session_id,
                mode: req.mode.as_str().to_string(),
                title,
                project_id: scope.project_id,
                task_id: scope.task_id,
                flow_id: scope.flow_id,
            },
            correlation,
        ),
        "server:chat:create",
    )?;

    Ok(map_session_inspect(&get_chat_session(
        service,
        &session_id.to_string(),
        "server:chat:create",
    )?))
}

#[allow(clippy::too_many_lines)]
fn send_chat_session_message(
    service: &ChatService,
    req: &ChatSessionSendRequest,
) -> Result<ChatSessionSendResponse> {
    let session = get_chat_session(service, &req.session_id, "server:chat:send")?;
    let message = validate_chat_message(
        req.message.as_str(),
        req.max_turns,
        req.timeout_ms,
        req.token_budget,
        "server:chat:send",
    )?;
    let request_id = Uuid::new_v4().to_string();
    let user_message_id = Uuid::new_v4();
    let assistant_message_id = Uuid::new_v4();
    let scope = resolve_chat_scope_from_session(service, &session, "server:chat:send")?;
    let correlation = chat_correlation(&scope);

    let default_title_prefix = format!("{} chat ", session.mode);
    if session.messages.is_empty() && session.title.starts_with(&default_title_prefix) {
        let title = preview_text(message, 48);
        service.append_event(
            Event::new(
                EventPayload::ChatSessionTitleUpdated {
                    session_id: session.id,
                    title,
                },
                correlation.clone(),
            ),
            "server:chat:send",
        )?;
    }

    service.append_event(
        Event::new(
            EventPayload::ChatMessageAppended {
                session_id: session.id,
                message_id: user_message_id,
                role: "user".to_string(),
                content: message.to_string(),
                request_id: Some(request_id.clone()),
                provider: None,
                model: None,
                final_state: None,
                runtime_selection_source: None,
            },
            correlation.clone(),
        ),
        "server:chat:send",
    )?;

    let history = session_history_inputs(&session);
    let response = execute_chat(
        service,
        &scope,
        &ChatExecutionRequest {
            mode: parse_mode(&session.mode),
            message,
            history: &history,
            context: req.context.as_deref(),
            provider: req.provider.as_deref(),
            model: req.model.as_deref(),
            max_turns: req.max_turns,
            timeout_ms: req.timeout_ms,
            token_budget: req.token_budget,
        },
        |turn| {
            let (directive_kind, directive_text) = directive_view(&turn.directive);
            service.append_event(
                Event::new(
                    EventPayload::ChatStreamChunkAppended {
                        session_id: session.id,
                        message_id: assistant_message_id,
                        request_id: request_id.clone(),
                        turn_index: turn.turn_index,
                        from_state: agent_state_label(turn.from_state).to_string(),
                        to_state: agent_state_label(turn.to_state).to_string(),
                        directive_kind: directive_kind.to_string(),
                        content: directive_text.to_string(),
                    },
                    correlation.clone(),
                ),
                "server:chat:send",
            )
        },
        "server:chat:send",
    )?;

    service.append_event(
        Event::new(
            EventPayload::ChatMessageAppended {
                session_id: session.id,
                message_id: assistant_message_id,
                role: "assistant".to_string(),
                content: response.assistant_message.clone(),
                request_id: Some(response.request_id.clone()),
                provider: Some(response.provider.clone()),
                model: Some(response.model.clone()),
                final_state: Some(response.final_state.clone()),
                runtime_selection_source: response.runtime_selection_source.clone(),
            },
            correlation,
        ),
        "server:chat:send",
    )?;

    Ok(ChatSessionSendResponse {
        session_id: session.id.to_string(),
        user_message_id: user_message_id.to_string(),
        assistant_message_id: assistant_message_id.to_string(),
        response,
    })
}
fn invoke_chat(service: &ChatService, req: &ChatInvokeRequest) -> Result<ChatInvokeResponse> {
    let message = validate_chat_message(
        req.message.as_str(),
        req.max_turns,
        req.timeout_ms,
        req.token_budget,
        "server:chat:invoke",
    )?;
    let scope = resolve_chat_scope(
        service,
        req.project.as_deref(),
        req.task.as_deref(),
        req.flow.as_deref(),
        "server:chat:invoke",
    )?;
    execute_chat(
        service,
        &scope,
        &ChatExecutionRequest {
            mode: req.mode,
            message,
            history: &req.history,
            context: req.context.as_deref(),
            provider: req.provider.as_deref(),
            model: req.model.as_deref(),
            max_turns: req.max_turns,
            timeout_ms: req.timeout_ms,
            token_budget: req.token_budget,
        },
        |_turn| Ok(()),
        "server:chat:invoke",
    )
}
