use super::scope::ResolvedChatScope;
use super::*;
use std::time::Duration;

pub(super) struct ChatExecutionRequest<'a> {
    pub(super) mode: ChatMode,
    pub(super) message: &'a str,
    pub(super) history: &'a [ChatHistoryMessageInput],
    pub(super) context: Option<&'a str>,
    pub(super) provider: Option<&'a str>,
    pub(super) model: Option<&'a str>,
    pub(super) max_turns: Option<u32>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) token_budget: Option<usize>,
}

pub(super) fn execute_chat<F>(
    service: &ChatService,
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
        .run_with_turn_callback(prompt, Some(context.as_str()), |turn| {
            on_turn(turn).map_err(|e| crate::native::NativeRuntimeError::ModelRequestFailed {
                code: e.code.clone(),
                message: e.message.clone(),
                recoverable: e.recoverable,
            })
        })
        .map_err(|error| error.to_hivemind_error(origin))?;
    let transport = loop_driver.take_transport_telemetry();
    let turns = map_turns(&result);
    let assistant_message = result
        .final_summary
        .clone()
        .or_else(|| turns.last().map(|turn| turn.directive_text.clone()))
        .unwrap_or_default();
    let _ = service;
    Ok(ChatInvokeResponse {
        request_id,
        mode: req.mode.as_str().to_string(),
        project_id: scope.project_id.map(|value: uuid::Uuid| value.to_string()),
        task_id: scope.task_id.map(|value: uuid::Uuid| value.to_string()),
        flow_id: scope.flow_id.map(|value: uuid::Uuid| value.to_string()),
        runtime_selection_source: scope
            .selection_source
            .map(|value: crate::core::events::RuntimeSelectionSource| value.as_str().to_string()),
        provider,
        model,
        assistant_message,
        final_state: agent_state_label(result.final_state).to_string(),
        turns,
        transport,
    })
}

pub(super) fn validate_chat_message<'a>(
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

pub(super) fn build_chat_config(
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
        ChatService::prepare_runtime_environment(&mut runtime, origin)?;
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

pub(super) fn build_model_client(
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

pub(super) fn build_chat_prompt(mode: ChatMode, message: &str) -> String {
    match mode {
        ChatMode::Plan => format!("You are in plan mode. Help the operator design a concrete Hivemind plan or flow, and end with a concise actionable summary.\n\nLatest user message:\n{message}"),
        ChatMode::Freeflow => format!("You are in free flow mode. Help the operator with open-ended assistance, and end with a concise actionable summary.\n\nLatest user message:\n{message}"),
    }
}

pub(super) fn build_chat_context(
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

pub(super) fn map_turns(result: &AgentLoopResult) -> Vec<ChatInvokeTurnView> {
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

pub(super) fn agent_state_label(state: AgentLoopState) -> &'static str {
    match state {
        AgentLoopState::Init => "init",
        AgentLoopState::Think => "think",
        AgentLoopState::Act => "act",
        AgentLoopState::Done => "done",
    }
}

pub(super) fn directive_view(directive: &ModelDirective) -> (&'static str, &str) {
    match directive {
        ModelDirective::Think { message } => ("think", message.as_str()),
        ModelDirective::Act { action } => ("act", action.as_str()),
        ModelDirective::Done { summary } => ("done", summary.as_str()),
    }
}
