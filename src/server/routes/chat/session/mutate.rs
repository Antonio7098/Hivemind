use super::super::execution::{
    agent_state_label, directive_view, execute_chat, validate_chat_message, ChatExecutionRequest,
};
use super::super::scope::{
    chat_correlation, default_session_title, get_chat_session, parse_mode, resolve_chat_scope,
    resolve_chat_scope_from_session, session_history_inputs,
};
use super::super::view::{map_session_inspect, preview_text};
use crate::app::ChatService;
use crate::core::events::{Event, EventPayload};
use crate::core::error::Result;
use crate::server::api_types::{
    ChatSessionCreateRequest, ChatSessionInspectView, ChatSessionSendRequest,
    ChatSessionSendResponse,
};
use uuid::Uuid;

pub(crate) fn create_chat_session(
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

// ARCH_DEBT: legacy oversized function
#[allow(clippy::too_many_lines)]
pub(crate) fn send_chat_session_message(
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
