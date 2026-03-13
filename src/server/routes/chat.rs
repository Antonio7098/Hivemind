use super::*;
use crate::core::events::RuntimeSelectionSource;
use crate::core::scope::RepoAccessMode;
use crate::core::state::{ChatMessageState, ChatSessionState, ProjectRuntimeConfig};
use crate::native::adapter::NativeAdapterConfig;
use crate::native::{
    AgentLoop, AgentLoopResult, AgentLoopState, AgentLoopTurn, MockModelClient, ModelClient,
    ModelDirective, OpenRouterModelClient,
};
use chrono::Utc;
use std::time::Duration;

const DEFAULT_CHAT_MODEL: &str = "openrouter/meta-llama/llama-3.2-3b-instruct:free";
const DEFAULT_CHAT_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone)]
struct ResolvedChatScope {
    project_id: Option<Uuid>,
    task_id: Option<Uuid>,
    flow_id: Option<Uuid>,
    project_summary: Option<String>,
    task_summary: Option<String>,
    flow_summary: Option<String>,
    runtime: Option<ProjectRuntimeConfig>,
    selection_source: Option<RuntimeSelectionSource>,
}

pub(super) fn handle_get(
    path: &str,
    url: &str,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/chat/sessions" => super::json_ok(list_chat_sessions(url, registry)?)?,
        "/api/chat/sessions/inspect" => super::json_ok(inspect_chat_session(url, registry)?)?,
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/chat/invoke" => {
            let req: ChatInvokeRequest = parse_json_body(body, "server:chat:invoke")?;
            super::json_ok(invoke_chat(registry, &req)?)?
        }
        "/api/chat/sessions/create" => {
            let req: ChatSessionCreateRequest = parse_json_body(body, "server:chat:create")?;
            super::json_ok(create_chat_session(registry, &req)?)?
        }
        "/api/chat/sessions/send" => {
            let req: ChatSessionSendRequest = parse_json_body(body, "server:chat:send")?;
            super::json_ok(send_chat_session_message(registry, &req)?)?
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

fn list_chat_sessions(url: &str, registry: &Registry) -> Result<Vec<ChatSessionSummaryView>> {
    let query = parse_query(url);
    let scope = resolve_chat_scope(
        registry,
        query.get("project").map(String::as_str),
        query.get("task").map(String::as_str),
        query.get("flow").map(String::as_str),
        "server:chat:list",
    )?;
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50);

    let state = registry.state()?;
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

fn inspect_chat_session(url: &str, registry: &Registry) -> Result<ChatSessionInspectView> {
    let query = parse_query(url);
    let session_id = query.get("session_id").ok_or_else(|| {
        HivemindError::user(
            "chat_session_id_required",
            "session_id is required",
            "server:chat:inspect",
        )
    })?;
    let session = get_chat_session(registry, session_id, "server:chat:inspect")?;
    Ok(map_session_inspect(&session))
}

fn create_chat_session(
    registry: &Registry,
    req: &ChatSessionCreateRequest,
) -> Result<ChatSessionInspectView> {
    let scope = resolve_chat_scope(
        registry,
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
    registry.append_event(
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
        registry,
        &session_id.to_string(),
        "server:chat:create",
    )?))
}

#[allow(clippy::too_many_lines)]
fn send_chat_session_message(
    registry: &Registry,
    req: &ChatSessionSendRequest,
) -> Result<ChatSessionSendResponse> {
    let session = get_chat_session(registry, &req.session_id, "server:chat:send")?;
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
    let scope = resolve_chat_scope_from_session(registry, &session, "server:chat:send")?;
    let correlation = chat_correlation(&scope);

    let default_title_prefix = format!("{} chat ", session.mode);
    if session.messages.is_empty() && session.title.starts_with(&default_title_prefix) {
        let title = preview_text(message, 48);
        registry.append_event(
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

    registry.append_event(
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
        registry,
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
            registry.append_event(
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

    registry.append_event(
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
fn invoke_chat(registry: &Registry, req: &ChatInvokeRequest) -> Result<ChatInvokeResponse> {
    let message = validate_chat_message(
        req.message.as_str(),
        req.max_turns,
        req.timeout_ms,
        req.token_budget,
        "server:chat:invoke",
    )?;
    let scope = resolve_chat_scope(
        registry,
        req.project.as_deref(),
        req.task.as_deref(),
        req.flow.as_deref(),
        "server:chat:invoke",
    )?;
    execute_chat(
        registry,
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

struct ChatExecutionRequest<'a> {
    mode: ChatMode,
    message: &'a str,
    history: &'a [ChatHistoryMessageInput],
    context: Option<&'a str>,
    provider: Option<&'a str>,
    model: Option<&'a str>,
    max_turns: Option<u32>,
    timeout_ms: Option<u64>,
    token_budget: Option<usize>,
}

fn execute_chat<F>(
    registry: &Registry,
    scope: &ResolvedChatScope,
    req: &ChatExecutionRequest<'_>,
    mut on_turn: F,
    origin: &'static str,
) -> Result<ChatInvokeResponse>
where
    F: FnMut(&AgentLoopTurn) -> Result<()>,
{
    let config = build_chat_config(
        scope.runtime.clone(),
        req.provider,
        req.model,
        req.max_turns,
        req.timeout_ms,
        req.token_budget,
        origin,
    )?;
    let prompt = build_chat_prompt(req.mode, req.message);
    let context = build_chat_context(req.mode, scope, req.history, req.context);
    let provider = config.provider_name.clone();
    let model = config.model_name.clone();
    let request_id = Uuid::new_v4().to_string();

    let model_client = build_model_client(&config, origin)?;
    let mut loop_driver = AgentLoop::new(config.native.clone(), model_client);
    let result = loop_driver
        .run(prompt, Some(context.as_str()))
        .map_err(|error| error.to_hivemind_error(origin))?;
    for turn in &result.turns {
        on_turn(turn)?;
    }
    let transport = loop_driver.take_transport_telemetry();
    let turns = map_turns(&result);
    let assistant_message = result
        .final_summary
        .clone()
        .or_else(|| turns.last().map(|turn| turn.directive_text.clone()))
        .unwrap_or_default();

    let _ = registry;
    Ok(ChatInvokeResponse {
        request_id,
        mode: req.mode.as_str().to_string(),
        project_id: scope.project_id.map(|value| value.to_string()),
        task_id: scope.task_id.map(|value| value.to_string()),
        flow_id: scope.flow_id.map(|value| value.to_string()),
        runtime_selection_source: scope
            .selection_source
            .map(|value| value.as_str().to_string()),
        provider,
        model,
        assistant_message,
        final_state: agent_state_label(result.final_state).to_string(),
        turns,
        transport,
    })
}

fn validate_chat_message<'a>(
    message: &'a str,
    max_turns: Option<u32>,
    timeout_ms: Option<u64>,
    token_budget: Option<usize>,
    origin: &'static str,
) -> Result<&'a str> {
    let message = message.trim();
    if message.is_empty() {
        return Err(HivemindError::user(
            "chat_message_required",
            "Chat message cannot be empty",
            origin,
        ));
    }
    if matches!(max_turns, Some(0)) {
        return Err(HivemindError::user(
            "invalid_chat_max_turns",
            "max_turns must be greater than zero",
            origin,
        ));
    }
    if matches!(timeout_ms, Some(0)) {
        return Err(HivemindError::user(
            "invalid_chat_timeout_ms",
            "timeout_ms must be greater than zero",
            origin,
        ));
    }
    if matches!(token_budget, Some(0)) {
        return Err(HivemindError::user(
            "invalid_chat_token_budget",
            "token_budget must be greater than zero",
            origin,
        ));
    }
    Ok(message)
}

fn resolve_chat_scope(
    registry: &Registry,
    project_ref: Option<&str>,
    task_ref: Option<&str>,
    flow_ref: Option<&str>,
    origin: &'static str,
) -> Result<ResolvedChatScope> {
    let state = registry.state()?;
    let explicit_project = match project_ref {
        Some(value) => Some(registry.get_project(value)?),
        None => None,
    };
    let task = match task_ref {
        Some(value) => Some(registry.get_task(value)?),
        None => None,
    };
    let flow = match flow_ref {
        Some(value) => Some(registry.get_flow(value)?),
        None => None,
    };

    let mut resolved_project_id = explicit_project.as_ref().map(|project| project.id);
    if let Some(task) = &task {
        ensure_same_project(&mut resolved_project_id, task.project_id, origin)?;
    }
    if let Some(flow) = &flow {
        ensure_same_project(&mut resolved_project_id, flow.project_id, origin)?;
    }

    let project = match (explicit_project, resolved_project_id) {
        (Some(project), _) => Some(project),
        (None, Some(project_id)) => state.projects.get(&project_id).cloned(),
        (None, None) => None,
    };
    let default_worker_runtime = state.global_runtime_defaults.worker;

    let runtime = if let Some(project) = &project {
        Registry::project_runtime_for_role_with_source(project, RuntimeRole::Worker).or_else(|| {
            default_worker_runtime
                .clone()
                .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault))
        })
    } else {
        default_worker_runtime.map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault))
    };
    let (runtime, selection_source) = runtime.map_or((None, None), |(runtime, source)| {
        (Some(runtime), Some(source))
    });

    Ok(ResolvedChatScope {
        project_id: project.as_ref().map(|value| value.id),
        task_id: task.as_ref().map(|value| value.id),
        flow_id: flow.as_ref().map(|value| value.id),
        project_summary: project.as_ref().map(summarize_project),
        task_summary: task.as_ref().map(summarize_task),
        flow_summary: flow.as_ref().map(summarize_flow),
        runtime,
        selection_source,
    })
}

fn resolve_chat_scope_from_session(
    registry: &Registry,
    session: &ChatSessionState,
    origin: &'static str,
) -> Result<ResolvedChatScope> {
    resolve_chat_scope(
        registry,
        session.project_id.map(|value| value.to_string()).as_deref(),
        session.task_id.map(|value| value.to_string()).as_deref(),
        session.flow_id.map(|value| value.to_string()).as_deref(),
        origin,
    )
}

fn ensure_same_project(
    slot: &mut Option<Uuid>,
    candidate: Uuid,
    origin: &'static str,
) -> Result<()> {
    match slot {
        Some(existing) if *existing != candidate => Err(HivemindError::user(
            "chat_scope_project_mismatch",
            "project/task/flow selection must belong to the same project",
            origin,
        )),
        Some(_) => Ok(()),
        None => {
            *slot = Some(candidate);
            Ok(())
        }
    }
}

fn get_chat_session(
    registry: &Registry,
    session_id: &str,
    origin: &'static str,
) -> Result<ChatSessionState> {
    let id = Uuid::parse_str(session_id).map_err(|_| {
        HivemindError::user(
            "invalid_chat_session_id",
            format!("'{session_id}' is not a valid chat session ID"),
            origin,
        )
    })?;
    let state = registry.state()?;
    state.chat_sessions.get(&id).cloned().ok_or_else(|| {
        HivemindError::user(
            "chat_session_not_found",
            format!("Chat session '{session_id}' not found"),
            origin,
        )
    })
}

fn session_history_inputs(session: &ChatSessionState) -> Vec<ChatHistoryMessageInput> {
    session
        .messages
        .iter()
        .map(|message| ChatHistoryMessageInput {
            role: if message.role == "assistant" {
                ChatHistoryRole::Assistant
            } else {
                ChatHistoryRole::User
            },
            content: message.content.clone(),
        })
        .collect()
}

fn summarize_project(project: &Project) -> String {
    let mut parts = vec![format!("Project: {} ({})", project.name, project.id)];
    if let Some(description) = project
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Description: {description}"));
    }
    if !project.repositories.is_empty() {
        let repos = project
            .repositories
            .iter()
            .map(|repo| {
                let access_mode = match repo.access_mode {
                    RepoAccessMode::ReadOnly => "read_only",
                    RepoAccessMode::ReadWrite => "read_write",
                };
                format!("{} @ {} [{}]", repo.name, repo.path, access_mode)
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Repositories: {repos}"));
    }
    parts.join("\n")
}

fn summarize_task(task: &Task) -> String {
    let mut parts = vec![format!("Task: {} ({})", task.title, task.id)];
    parts.push(format!(
        "Task state: {:?}; run mode: {:?}",
        task.state, task.run_mode
    ));
    if let Some(description) = task
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Task description: {description}"));
    }
    parts.join("\n")
}

fn summarize_flow(flow: &TaskFlow) -> String {
    let mut counts = HashMap::<String, usize>::new();
    for execution in flow.task_executions.values() {
        *counts
            .entry(format!("{:?}", execution.state).to_lowercase())
            .or_default() += 1;
    }
    let mut states = counts.into_iter().collect::<Vec<_>>();
    states.sort_by(|a, b| a.0.cmp(&b.0));
    let summary = states
        .into_iter()
        .map(|(state, count)| format!("{state}:{count}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Flow: {}\nFlow state: {:?}; run mode: {:?}\nTask execution states: {}",
        flow.id,
        flow.state,
        flow.run_mode,
        if summary.is_empty() { "none" } else { &summary }
    )
}

fn build_chat_config(
    runtime: Option<ProjectRuntimeConfig>,
    provider: Option<&str>,
    model: Option<&str>,
    max_turns: Option<u32>,
    timeout_ms: Option<u64>,
    token_budget: Option<usize>,
    origin: &'static str,
) -> Result<NativeAdapterConfig> {
    let mut config = NativeAdapterConfig::new();
    config.provider_name = std::env::var("HIVEMIND_NATIVE_PROVIDER")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "openrouter".to_string());
    config.model_name = std::env::var("HIVEMIND_CHAT_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CHAT_MODEL.to_string());

    if let Some(mut runtime) = runtime {
        Registry::prepare_runtime_environment(&mut runtime, origin)?;
        let timeout = Duration::from_millis(runtime.timeout_ms.max(1));
        config.base.env = runtime.env;
        config.base.args.clone_from(&runtime.args);
        config.base.timeout = timeout;
        config.native.timeout_budget = timeout;

        if runtime.adapter_name.eq_ignore_ascii_case("native") {
            if let Some(model_name) = runtime.model.filter(|value| !value.trim().is_empty()) {
                config.model_name = model_name;
            }
            if let Some(provider_name) = config.base.env.get("HIVEMIND_NATIVE_PROVIDER").cloned() {
                config.provider_name = provider_name;
            }
            if !runtime.args.is_empty() {
                config.scripted_directives = runtime.args;
            }
        }
    }

    if let Some(value) = provider.map(str::trim).filter(|value| !value.is_empty()) {
        config.provider_name = value.to_string();
    }
    if let Some(value) = model.map(str::trim).filter(|value| !value.is_empty()) {
        config.model_name = value.to_string();
    }
    if let Some(value) = timeout_ms {
        let timeout = Duration::from_millis(value.max(1));
        config.base.timeout = timeout;
        config.native.timeout_budget = timeout;
    } else if config.base.timeout.is_zero() {
        let timeout = Duration::from_millis(DEFAULT_CHAT_TIMEOUT_MS);
        config.base.timeout = timeout;
        config.native.timeout_budget = timeout;
    }
    if let Some(value) = max_turns {
        config.native.max_turns = value;
    }
    if let Some(value) = token_budget {
        config.native.token_budget = value;
    }
    Ok(config)
}

fn build_model_client(
    config: &NativeAdapterConfig,
    origin: &'static str,
) -> Result<Box<dyn ModelClient>> {
    if config.provider_name.eq_ignore_ascii_case("openrouter") {
        let client = OpenRouterModelClient::from_env(config.model_name.clone(), &config.base.env)
            .map_err(|error| error.to_hivemind_error(origin))?;
        Ok(Box::new(client))
    } else {
        let client = if config.scripted_directives.is_empty() {
            MockModelClient::deterministic_default()
        } else {
            MockModelClient::from_outputs(config.scripted_directives.clone())
        };
        Ok(Box::new(client))
    }
}

fn build_chat_prompt(mode: ChatMode, message: &str) -> String {
    match mode {
        ChatMode::Plan => format!(
            "You are in plan mode. Help the operator design a concrete Hivemind plan or flow, and end with a concise actionable summary.\n\nLatest user message:\n{message}"
        ),
        ChatMode::Freeflow => format!(
            "You are in free flow mode. Help the operator with open-ended assistance, and end with a concise actionable summary.\n\nLatest user message:\n{message}"
        ),
    }
}

fn build_chat_context(
    mode: ChatMode,
    scope: &ResolvedChatScope,
    history: &[ChatHistoryMessageInput],
    extra_context: Option<&str>,
) -> String {
    let mut sections = vec![match mode {
        ChatMode::Plan => {
            "Mode notes: prioritize sequencing, risks, validation, and next-step planning."
                .to_string()
        }
        ChatMode::Freeflow => {
            "Mode notes: prioritize direct operator assistance and lightweight guidance."
                .to_string()
        }
    }];

    if let Some(value) = scope.project_summary.as_deref() {
        sections.push(value.to_string());
    }
    if let Some(value) = scope.task_summary.as_deref() {
        sections.push(value.to_string());
    }
    if let Some(value) = scope.flow_summary.as_deref() {
        sections.push(value.to_string());
    }

    if !history.is_empty() {
        let transcript = history
            .iter()
            .map(|entry| {
                format!(
                    "{}: {}",
                    entry.role.as_str().to_uppercase(),
                    entry.content.trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(format!("Conversation history:\n{transcript}"));
    }

    if let Some(value) = extra_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        sections.push(format!("Additional context:\n{value}"));
    }

    sections.join("\n\n")
}

fn map_turns(result: &AgentLoopResult) -> Vec<ChatInvokeTurnView> {
    result
        .turns
        .iter()
        .map(|turn| {
            let (directive_kind, directive_text) = directive_view(&turn.directive);
            ChatInvokeTurnView {
                turn_index: turn.turn_index,
                from_state: agent_state_label(turn.from_state).to_string(),
                to_state: agent_state_label(turn.to_state).to_string(),
                directive_kind: directive_kind.to_string(),
                directive_text: directive_text.to_string(),
                raw_output: turn.raw_output.clone(),
            }
        })
        .collect()
}

fn directive_view(directive: &ModelDirective) -> (&'static str, &str) {
    match directive {
        ModelDirective::Think { message } => ("think", message.as_str()),
        ModelDirective::Act { action } => ("act", action.as_str()),
        ModelDirective::Done { summary } => ("done", summary.as_str()),
    }
}

fn map_session_summary(session: &ChatSessionState) -> ChatSessionSummaryView {
    ChatSessionSummaryView {
        session_id: session.id.to_string(),
        mode: session.mode.clone(),
        title: session.title.clone(),
        project_id: session.project_id.map(|value| value.to_string()),
        task_id: session.task_id.map(|value| value.to_string()),
        flow_id: session.flow_id.map(|value| value.to_string()),
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
        message_count: session.messages.len(),
        last_message_preview: session
            .messages
            .last()
            .map(|message| preview_text(&message.content, 96)),
    }
}

fn map_session_inspect(session: &ChatSessionState) -> ChatSessionInspectView {
    ChatSessionInspectView {
        session_id: session.id.to_string(),
        mode: session.mode.clone(),
        title: session.title.clone(),
        project_id: session.project_id.map(|value| value.to_string()),
        task_id: session.task_id.map(|value| value.to_string()),
        flow_id: session.flow_id.map(|value| value.to_string()),
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
        messages: session.messages.iter().map(map_session_message).collect(),
    }
}

fn map_session_message(message: &ChatMessageState) -> ChatSessionMessageView {
    ChatSessionMessageView {
        message_id: message.id.to_string(),
        role: message.role.clone(),
        content: message.content.clone(),
        created_at: message.created_at.to_rfc3339(),
        request_id: message.request_id.clone(),
        provider: message.provider.clone(),
        model: message.model.clone(),
        final_state: message.final_state.clone(),
        runtime_selection_source: message.runtime_selection_source.clone(),
    }
}

fn preview_text(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let preview = trimmed.chars().take(max_chars).collect::<String>();
    format!("{preview}…")
}

fn default_session_title(mode: ChatMode) -> String {
    format!(
        "{} chat {}",
        mode.as_str(),
        Utc::now().format("%Y-%m-%d %H:%M")
    )
}

fn parse_mode(raw: &str) -> ChatMode {
    if raw.eq_ignore_ascii_case("freeflow") {
        ChatMode::Freeflow
    } else {
        ChatMode::Plan
    }
}

fn chat_correlation(scope: &ResolvedChatScope) -> CorrelationIds {
    match (scope.project_id, scope.flow_id, scope.task_id) {
        (Some(project_id), Some(flow_id), Some(task_id)) => {
            CorrelationIds::for_flow_task(project_id, flow_id, task_id)
        }
        (Some(project_id), Some(flow_id), None) => CorrelationIds::for_flow(project_id, flow_id),
        (Some(project_id), None, Some(task_id)) => CorrelationIds::for_task(project_id, task_id),
        (Some(project_id), None, None) => CorrelationIds::for_project(project_id),
        _ => CorrelationIds::none(),
    }
}

fn agent_state_label(state: AgentLoopState) -> &'static str {
    match state {
        AgentLoopState::Init => "init",
        AgentLoopState::Think => "think",
        AgentLoopState::Act => "act",
        AgentLoopState::Done => "done",
    }
}
