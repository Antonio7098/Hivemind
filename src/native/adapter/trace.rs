use super::*;
use crate::adapters::runtime::NativeHistoryCompactionTrace;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidationAttempt {
    command_line: String,
    success: bool,
    detail: String,
}

fn split_inline_command(raw: &str) -> Option<(String, Vec<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let command = parts.next()?.to_string();
    let args = parts.map(ToString::to_string).collect::<Vec<_>>();
    if args.is_empty() {
        None
    } else {
        Some((command, args))
    }
}

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
                    active_code_window_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_chars)
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
                    active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_count)
                        .unwrap_or_default(),
                    active_code_window_trace: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_trace.clone())
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
                    omitted_active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.omitted_active_code_window_count)
                        .unwrap_or_default(),
                    suppressed_by_active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_by_active_code_window_count)
                        .unwrap_or_default(),
                    suppressed_duplicate_read_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_duplicate_read_count)
                        .unwrap_or_default(),
                    suppressed_stale_tool_call_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_stale_tool_call_count)
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
        let failure = Self::success_guard_failure(&turns, result.final_summary.as_deref());

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
            failure,
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
                    active_code_window_chars: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_chars)
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
                    active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_count)
                        .unwrap_or_default(),
                    active_code_window_trace: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.active_code_window_trace.clone())
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
                    omitted_active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.omitted_active_code_window_count)
                        .unwrap_or_default(),
                    suppressed_by_active_code_window_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_by_active_code_window_count)
                        .unwrap_or_default(),
                    suppressed_duplicate_read_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_duplicate_read_count)
                        .unwrap_or_default(),
                    suppressed_stale_tool_call_count: turn
                        .request
                        .prompt_assembly
                        .as_ref()
                        .map(|assembly| assembly.suppressed_stale_tool_call_count)
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

    fn success_guard_failure(
        turns: &[NativeTurnTrace],
        final_summary: Option<&str>,
    ) -> Option<NativeInvocationFailure> {
        if let Some((turn_index, tool_name, failure)) = turns.iter().find_map(|turn| {
            turn.tool_calls.iter().find_map(|tool_call| {
                tool_call.failure.as_ref().and_then(|failure| {
                    (!failure.recoverable).then_some((
                        turn.turn_index,
                        &tool_call.tool_name,
                        failure,
                    ))
                })
            })
        }) {
            return Some(NativeInvocationFailure {
                code: "native_nonrecoverable_tool_failure".to_string(),
                message: format!(
                    "turn {turn_index} tool '{tool_name}' failed non-recoverably: {} ({})",
                    failure.message, failure.code
                ),
                recoverable: false,
                recovery_hint: Some(
                    "Resolve the hard tool failure before accepting a successful native completion."
                        .to_string(),
                ),
            });
        }

        let validation_attempts = Self::validation_attempts(turns);
        let saw_validation = !validation_attempts.is_empty();
        let saw_successful_validation = validation_attempts.iter().any(|attempt| attempt.success);
        if saw_validation
            && !saw_successful_validation
            && Self::summary_claims_validation(final_summary)
        {
            let detail = validation_attempts
                .into_iter()
                .map(|attempt| format!("{} [{}]", attempt.command_line, attempt.detail))
                .collect::<Vec<_>>()
                .join(", ");
            return Some(NativeInvocationFailure {
                code: "native_unverified_validation_claim".to_string(),
                message: format!(
                    "native summary claimed verification, but no validation command succeeded: {detail}"
                ),
                recoverable: false,
                recovery_hint: Some(
                    "Run a focused validation command successfully before returning DONE."
                        .to_string(),
                ),
            });
        }

        None
    }

    fn validation_attempts(turns: &[NativeTurnTrace]) -> Vec<ValidationAttempt> {
        turns
            .iter()
            .flat_map(|turn| turn.tool_calls.iter())
            .filter_map(|tool_call| {
                let command_line = Self::tool_request_command_line(tool_call)?;
                if !Self::is_validation_command(&command_line) {
                    return None;
                }
                if let Some(failure) = tool_call.failure.as_ref() {
                    return Some(ValidationAttempt {
                        command_line,
                        success: false,
                        detail: format!("tool_failed:{}", failure.code),
                    });
                }
                let exit_code = Self::tool_response_exit_code(tool_call)?;
                Some(ValidationAttempt {
                    command_line,
                    success: exit_code == 0,
                    detail: format!("exit_code:{exit_code}"),
                })
            })
            .collect()
    }

    fn tool_request_command_line(tool_call: &NativeToolCallTrace) -> Option<String> {
        let request: Value = serde_json::from_str(&tool_call.request).ok()?;
        let input = request.get("input")?.as_object()?;
        let primary_key = match tool_call.tool_name.as_str() {
            "run_command" => "command",
            "exec_command" => "cmd",
            _ => return None,
        };
        let mut command = input.get(primary_key)?.as_str()?.trim().to_string();
        let mut args = input
            .get("args")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if args.is_empty() {
            if let Some((split_command, split_args)) = split_inline_command(&command) {
                command = split_command;
                args = split_args;
            }
        }
        if args.is_empty() {
            Some(command)
        } else {
            Some(format!("{} {}", command, args.join(" ")))
        }
    }

    fn tool_response_exit_code(tool_call: &NativeToolCallTrace) -> Option<i64> {
        let response = tool_call.response.as_ref()?;
        let payload: Value = serde_json::from_str(response).ok()?;
        payload.get("output")?.get("exit_code")?.as_i64()
    }

    fn is_validation_command(command_line: &str) -> bool {
        let lowered = command_line.trim().to_ascii_lowercase();
        matches!(
            lowered.split_whitespace().collect::<Vec<_>>().as_slice(),
            ["cargo", "test", ..]
                | ["cargo", "check", ..]
                | ["cargo", "clippy", ..]
                | ["cargo", "fmt", ..]
                | ["pytest", ..]
                | ["py.test", ..]
                | ["go", "test", ..]
                | ["just", "test", ..]
                | ["npm", "test", ..]
                | ["pnpm", "test", ..]
                | ["yarn", "test", ..]
                | ["bun", "test", ..]
        )
    }

    fn summary_claims_validation(final_summary: Option<&str>) -> bool {
        let Some(summary) = final_summary
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
        else {
            return false;
        };
        let lowered = summary.to_ascii_lowercase();
        [
            "verified",
            "verification",
            "tests passed",
            "cargo test",
            "validated",
        ]
        .iter()
        .any(|needle| lowered.contains(needle))
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
                prompt_response: None,
                response_original_bytes: None,
                response_stored_bytes: None,
                response_truncated: false,
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
                prompt_response: None,
                response_original_bytes: Some(action.len().saturating_add(10)),
                response_stored_bytes: Some(action.len().saturating_add(10)),
                response_truncated: false,
                failure: None,
                policy_tags: Vec::new(),
            },
            Err(error) => NativeToolCallTrace {
                call_id,
                tool_name: "native_action".to_string(),
                request: action.to_string(),
                duration_ms: None,
                response: None,
                prompt_response: None,
                response_original_bytes: None,
                response_stored_bytes: None,
                response_truncated: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_call(
        tool_name: &str,
        request: Value,
        response: Option<Value>,
        failure: Option<NativeToolCallFailure>,
    ) -> NativeToolCallTrace {
        NativeToolCallTrace {
            call_id: format!("call-{tool_name}"),
            tool_name: tool_name.to_string(),
            request: serde_json::to_string(&request).expect("serialize request"),
            duration_ms: Some(1),
            response: response
                .map(|value| serde_json::to_string(&value).expect("serialize response")),
            prompt_response: None,
            response_original_bytes: None,
            response_stored_bytes: None,
            response_truncated: false,
            failure,
            policy_tags: Vec::new(),
        }
    }

    fn turn(turn_index: u32, tool_calls: Vec<NativeToolCallTrace>) -> NativeTurnTrace {
        NativeTurnTrace {
            turn_index,
            from_state: "act".to_string(),
            to_state: "done".to_string(),
            agent_mode: Some("task_executor".to_string()),
            model_request: "{}".to_string(),
            model_response: String::new(),
            prompt_hash: None,
            context_manifest_hash: None,
            delivered_context_hash: None,
            prompt_headroom: None,
            available_budget: 0,
            mode_contract_hash: None,
            inputs_hash: None,
            rendered_context_hash: None,
            context_window_state_hash: None,
            delivery_target: None,
            rendered_prompt_bytes: 0,
            runtime_context_bytes: 0,
            static_prompt_chars: 0,
            selected_history_chars: 0,
            active_code_window_chars: 0,
            compacted_summary_chars: 0,
            code_navigation_chars: 0,
            tool_contract_chars: 0,
            assembly_duration_ms: 0,
            visible_item_count: 0,
            selected_history_count: 0,
            active_code_window_count: 0,
            active_code_window_trace: Vec::new(),
            code_navigation_count: 0,
            compacted_summary_count: 0,
            tool_contract_count: 0,
            skipped_item_count: 0,
            truncated_item_count: 0,
            omitted_active_code_window_count: 0,
            suppressed_by_active_code_window_count: 0,
            suppressed_duplicate_read_count: 0,
            suppressed_stale_tool_call_count: 0,
            tool_result_items_visible: 0,
            latest_tool_result_turn_index: None,
            latest_tool_names_visible: Vec::new(),
            tool_result_count: tool_calls.len(),
            budget_used_before: 0,
            budget_used_after: 0,
            budget_remaining: 0,
            budget_thresholds_crossed: Vec::new(),
            model_latency_ms: 0,
            tool_latency_ms: 0,
            turn_duration_ms: 0,
            elapsed_since_invocation_ms: 0,
            request_tokens: 0,
            response_tokens: 0,
            tool_calls,
            turn_summary: None,
        }
    }

    #[test]
    fn success_guard_flags_nonrecoverable_tool_failures() {
        let turns = vec![turn(
            7,
            vec![tool_call(
                "run_command",
                serde_json::json!({
                    "tool": "run_command",
                    "input": { "command": "cargo test" }
                }),
                None,
                Some(NativeToolCallFailure {
                    code: "native_tool_execution_failed".to_string(),
                    message: "boom".to_string(),
                    recoverable: false,
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: None,
                }),
            )],
        )];

        let failure =
            NativeRuntimeAdapter::success_guard_failure(&turns, Some("verified with cargo test"))
                .expect("failure should be produced");
        assert_eq!(failure.code, "native_nonrecoverable_tool_failure");
    }

    #[test]
    fn success_guard_ignores_recoverable_tool_failures_when_completion_is_valid() {
        let turns = vec![turn(
            7,
            vec![tool_call(
                "read_file",
                serde_json::json!({
                    "tool": "read_file",
                    "input": { "path": "src/native/turn_items.rs" }
                }),
                None,
                Some(NativeToolCallFailure {
                    code: "native_tool_input_invalid".to_string(),
                    message: "missing path".to_string(),
                    recoverable: true,
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: Some("Include a valid path".to_string()),
                }),
            )],
        )];

        let failure =
            NativeRuntimeAdapter::success_guard_failure(&turns, Some("verified with cargo test"));
        assert!(failure.is_none());
    }

    #[test]
    fn success_guard_flags_unverified_validation_claims() {
        let turns = vec![turn(
            3,
            vec![tool_call(
                "run_command",
                serde_json::json!({
                    "tool": "run_command",
                    "input": { "command": "cargo test" }
                }),
                Some(serde_json::json!({
                    "ok": true,
                    "output": { "exit_code": 101 }
                })),
                None,
            )],
        )];

        let failure = NativeRuntimeAdapter::success_guard_failure(
            &turns,
            Some("Implemented changes and verified with cargo test"),
        )
        .expect("failure should be produced");
        assert_eq!(failure.code, "native_unverified_validation_claim");
    }
}
