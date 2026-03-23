use super::*;
use crate::app::ChatService;
use crate::core::events::RuntimeSelectionSource;

mod execution;
mod scope;
mod session;
mod view;
use crate::core::scope::RepoAccessMode;
use crate::core::state::{ChatMessageState, ChatSessionState, ProjectRuntimeConfig};
use crate::native::adapter::NativeAdapterConfig;
use crate::native::{
    AgentLoop, AgentLoopResult, AgentLoopState, AgentLoopTurn, MockModelClient, ModelClient,
    ModelDirective, OpenRouterModelClient,
};
use chrono::Utc;
use execution::{execute_chat, validate_chat_message, ChatExecutionRequest};
use scope::resolve_chat_scope;
use session::{
    mutate::{create_chat_session, send_chat_session_message},
    query::{inspect_chat_session, list_chat_sessions},
};

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
