use super::*;
use crate::adapters::runtime::NativeHistoryCompactionTrace;

impl NativeRuntimeAdapter {
    pub(crate) fn render_stdout(result: &crate::native::AgentLoopResult) -> String {
        let mut lines = Vec::new();
        for turn in &result.turns {
            let directive = match &turn.directive {
                crate::native::ModelDirective::Think { message } => format!("THINK {message}"),
                crate::native::ModelDirective::Act { action } => format!("ACT {action}"),
                crate::native::ModelDirective::Done { summary } => format!("DONE {summary}"),
            };
            lines.push(format!(
                "native turn {}: {} -> {} | {directive}",
                turn.turn_index,
                turn.from_state.as_str(),
                turn.to_state.as_str(),
            ));
        }
        if let Some(summary) = result.final_summary.as_ref() {
            lines.push(format!("native summary: {summary}"));
        }
        lines.join("\n")
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn trace_from_success(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        result: &crate::native::AgentLoopResult,
        transport: NativeTransportTelemetry,
        runtime_state: Option<NativeRuntimeStateTelemetry>,
        readiness_transitions: Vec<NativeReadinessTransition>,
        history_compactions: Vec<NativeHistoryCompactionTrace>,
        allowed_tools: Vec<String>,
        allowed_capabilities: Vec<String>,
    ) -> NativeInvocationTrace {
        let turns: Vec<NativeTurnTrace> = result
            .turns
            .iter()
            .map(|turn| {
                let model_request = serde_json::to_string(&turn.request).unwrap_or_else(|_| {
                    format!(
                        "turn={} state={} prompt={}",
                        turn.request.turn_index,
                        turn.request.state.as_str(),
                        turn.request.prompt
                    )
                });

                let turn_summary = if let ModelDirective::Done { summary } = &turn.directive {
                    Some(summary.clone())
                } else {
                    None
                };

                NativeTurnTrace {
                    turn_index: turn.turn_index,
                    from_state: turn.from_state.as_str().to_string(),
                    to_state: turn.to_state.as_str().to_string(),
                    agent_mode: Some(turn.request.agent_mode.as_str().to_string()),
                    model_request,
                    model_response: turn.raw_output.clone(),
                    prompt_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.rendered_prompt_hash.clone()),
                    context_manifest_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.manifest_hash.clone()),
                    delivered_context_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.delivered_context_hash.clone()),
                    prompt_headroom: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.prompt_headroom),
                    available_budget: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.available_budget)
                        .unwrap_or_default(),
                    mode_contract_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.mode_contract_hash.clone()),
                    inputs_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.inputs_hash.clone()),
                    rendered_context_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.rendered_context_hash.clone()),
                    context_window_state_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.context_window_state_hash.clone()),
                    delivery_target: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.delivery_target.clone()),
                    rendered_prompt_bytes: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.rendered_prompt_bytes)
                        .unwrap_or_default(),
                    runtime_context_bytes: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.runtime_context_bytes)
                        .unwrap_or_default(),
                    static_prompt_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.static_prompt_chars)
                        .unwrap_or_default(),
                    selected_history_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_history_chars)
                        .unwrap_or_default(),
                    compacted_summary_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.compacted_summary_chars)
                        .unwrap_or_default(),
                    code_navigation_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.code_navigation_chars)
                        .unwrap_or_default(),
                    tool_contract_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_contract_chars)
                        .unwrap_or_default(),
                    assembly_duration_ms: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.assembly_duration_ms)
                        .unwrap_or_default(),
                    visible_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_item_count)
                        .unwrap_or_default(),
                    selected_history_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_history_count)
                        .unwrap_or_default(),
                    code_navigation_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.code_navigation_count)
                        .unwrap_or_default(),
                    compacted_summary_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.compacted_summary_count)
                        .unwrap_or_default(),
                    tool_contract_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_contract_count)
                        .unwrap_or_default(),
                    skipped_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.skipped_item_count)
                        .unwrap_or_default(),
                    truncated_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.truncated_item_count)
                        .unwrap_or_default(),
                    tool_result_items_visible: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_result_items_visible)
                        .unwrap_or_default(),
                    latest_tool_result_turn_index: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.latest_tool_result_turn_index),
                    latest_tool_names_visible: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.latest_tool_names_visible.clone())
                        .unwrap_or_default(),
                    tool_result_count: turn.tool_calls.len(),
                    budget_used_before: turn.budget_used_before,
                    budget_used_after: turn.budget_used_after,
                    budget_remaining: turn.budget_remaining,
                    budget_thresholds_crossed: turn.budget_thresholds_crossed.clone(),
                    model_latency_ms: turn.model_latency_ms,
                    tool_latency_ms: turn.tool_latency_ms,
                    turn_duration_ms: turn.turn_duration_ms,
                    elapsed_since_invocation_ms: turn.elapsed_since_invocation_ms,
                    request_tokens: turn.request_tokens,
                    response_tokens: turn.response_tokens,
                    tool_calls: turn.tool_calls.clone(),
                    turn_summary,
                }
            })
            .collect();

        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            agent_mode: Some(config.native.agent_mode.as_str().to_string()),
            allowed_tools,
            allowed_capabilities,
            configured_max_turns: Some(config.native.max_turns),
            configured_timeout_budget_ms: Some(
                u64::try_from(config.native.timeout_budget.as_millis()).unwrap_or(u64::MAX),
            ),
            configured_token_budget: Some(config.native.token_budget),
            configured_prompt_headroom: Some(config.native.prompt_headroom),
            turns,
            final_state: result.final_state.as_str().to_string(),
            final_summary: result.final_summary.clone(),
            failure: None,
            transport,
            runtime_state,
            readiness_transitions,
            history_compactions,
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub(crate) fn trace_from_failure(
        invocation_id: &str,
        config: &NativeAdapterConfig,
        result: &crate::native::AgentLoopResult,
        final_state: crate::native::AgentLoopState,
        err: &crate::native::NativeRuntimeError,
        transport: NativeTransportTelemetry,
        runtime_state: Option<NativeRuntimeStateTelemetry>,
        readiness_transitions: Vec<NativeReadinessTransition>,
        history_compactions: Vec<NativeHistoryCompactionTrace>,
        allowed_tools: Vec<String>,
        allowed_capabilities: Vec<String>,
    ) -> NativeInvocationTrace {
        let turns: Vec<NativeTurnTrace> = result
            .turns
            .iter()
            .map(|turn| {
                let model_request = serde_json::to_string(&turn.request).unwrap_or_else(|_| {
                    format!(
                        "turn={} state={} prompt={}",
                        turn.request.turn_index,
                        turn.request.state.as_str(),
                        turn.request.prompt
                    )
                });

                let turn_summary = if let ModelDirective::Done { summary } = &turn.directive {
                    Some(summary.clone())
                } else {
                    None
                };

                NativeTurnTrace {
                    turn_index: turn.turn_index,
                    from_state: turn.from_state.as_str().to_string(),
                    to_state: turn.to_state.as_str().to_string(),
                    agent_mode: Some(turn.request.agent_mode.as_str().to_string()),
                    model_request,
                    model_response: turn.raw_output.clone(),
                    prompt_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.rendered_prompt_hash.clone()),
                    context_manifest_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.manifest_hash.clone()),
                    delivered_context_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.delivered_context_hash.clone()),
                    prompt_headroom: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.prompt_headroom),
                    available_budget: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.available_budget)
                        .unwrap_or_default(),
                    mode_contract_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.mode_contract_hash.clone()),
                    inputs_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.inputs_hash.clone()),
                    rendered_context_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.rendered_context_hash.clone()),
                    context_window_state_hash: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.context_window_state_hash.clone()),
                    delivery_target: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.delivery_target.clone()),
                    rendered_prompt_bytes: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.rendered_prompt_bytes)
                        .unwrap_or_default(),
                    runtime_context_bytes: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.runtime_context_bytes)
                        .unwrap_or_default(),
                    static_prompt_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.static_prompt_chars)
                        .unwrap_or_default(),
                    selected_history_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_history_chars)
                        .unwrap_or_default(),
                    compacted_summary_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.compacted_summary_chars)
                        .unwrap_or_default(),
                    code_navigation_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.code_navigation_chars)
                        .unwrap_or_default(),
                    tool_contract_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_contract_chars)
                        .unwrap_or_default(),
                    assembly_duration_ms: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.assembly_duration_ms)
                        .unwrap_or_default(),
                    visible_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_item_count)
                        .unwrap_or_default(),
                    selected_history_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.selected_history_count)
                        .unwrap_or_default(),
                    code_navigation_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.code_navigation_count)
                        .unwrap_or_default(),
                    compacted_summary_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.compacted_summary_count)
                        .unwrap_or_default(),
                    tool_contract_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_contract_count)
                        .unwrap_or_default(),
                    skipped_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.skipped_item_count)
                        .unwrap_or_default(),
                    truncated_item_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.truncated_item_count)
                        .unwrap_or_default(),
                    tool_result_items_visible: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.tool_result_items_visible)
                        .unwrap_or_default(),
                    latest_tool_result_turn_index: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .and_then(|assembly| assembly.latest_tool_result_turn_index),
                    latest_tool_names_visible: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.latest_tool_names_visible.clone())
                        .unwrap_or_default(),
                    tool_result_count: turn.tool_calls.len(),
                    budget_used_before: turn.budget_used_before,
                    budget_used_after: turn.budget_used_after,
                    budget_remaining: turn.budget_remaining,
                    budget_thresholds_crossed: turn.budget_thresholds_crossed.clone(),
                    model_latency_ms: turn.model_latency_ms,
                    tool_latency_ms: turn.tool_latency_ms,
                    turn_duration_ms: turn.turn_duration_ms,
                    elapsed_since_invocation_ms: turn.elapsed_since_invocation_ms,
                    request_tokens: turn.request_tokens,
                    response_tokens: turn.response_tokens,
                    tool_calls: turn.tool_calls.clone(),
                    turn_summary,
                }
            })
            .collect();

        NativeInvocationTrace {
            invocation_id: invocation_id.to_string(),
            provider: config.provider_name.clone(),
            model: config.model_name.clone(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            capture_mode: Self::capture_mode(&config.native),
            agent_mode: Some(config.native.agent_mode.as_str().to_string()),
            allowed_tools,
            allowed_capabilities,
            configured_max_turns: Some(config.native.max_turns),
            configured_timeout_budget_ms: Some(
                u64::try_from(config.native.timeout_budget.as_millis()).unwrap_or(u64::MAX),
            ),
            configured_token_budget: Some(config.native.token_budget),
            configured_prompt_headroom: Some(config.native.prompt_headroom),
            turns,
            final_state: final_state.as_str().to_string(),
            final_summary: result.final_summary.clone(),
            failure: Some(NativeInvocationFailure {
                code: err.code().to_string(),
                message: err.message(),
                recoverable: err.recoverable(),
                recovery_hint: err.recovery_hint(),
            }),
            transport,
            runtime_state,
            readiness_transitions,
            history_compactions,
        }
    }

    pub(super) fn tool_trace_for_act(
        invocation_id: &str,
        agent_mode: crate::native::AgentMode,
        turn_index: u32,
        action: &str,
        tool_engine: &NativeToolEngine,
        tool_context: &ToolExecutionContext<'_>,
    ) -> NativeToolCallTrace {
        let call_id = format!("{invocation_id}:turn:{turn_index}:tool:0");
        if let Some(message) = action.strip_prefix("fail_tool:") {
            return NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                duration_ms: None,
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: "native_tool_call_failed".to_string(),
                    message: message.trim().to_string(),
                    recoverable: false,
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: None,
                }),
                policy_tags: Vec::new(),
            };
        }

        match NativeToolAction::parse(action) {
            Ok(Some(tool_action)) => tool_engine.execute_action_trace_for_mode(
                agent_mode,
                call_id,
                &tool_action,
                tool_context,
            ),
            Ok(None) => NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                duration_ms: None,
                response: Some(format!("completed:{action}")),
                failure: None,
                policy_tags: Vec::new(),
            },
            Err(error) => NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                duration_ms: None,
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: error.code,
                    message: error.message,
                    recoverable: error.recoverable,
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: None,
                }),
                policy_tags: error.policy_tags,
            },
        }
    }
}
