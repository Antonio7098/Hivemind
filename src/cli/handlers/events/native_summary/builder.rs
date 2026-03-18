use super::types::*;
use crate::core::events::{Event, EventPayload};
use std::collections::{BTreeMap, BTreeSet};

#[allow(clippy::too_many_lines)]
pub(super) fn build_native_summary(events: &[Event]) -> NativeSummaryReport {
    let mut invocations = BTreeMap::<String, NativeInvocationSummary>::new();
    let mut verification = NativeVerificationSummary::default();
    for event in events {
        match &event.payload {
            EventPayload::AgentInvocationStarted {
                native_correlation,
                invocation_id,
                adapter_name,
                provider,
                model,
                agent_mode,
                allowed_tools,
                allowed_capabilities,
                configured_max_turns,
                configured_timeout_budget_ms,
                configured_token_budget,
                configured_prompt_headroom,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                entry.invocation_id.clone_from(invocation_id);
                entry.project_id = Some(native_correlation.project_id.to_string());
                entry.graph_id = Some(native_correlation.graph_id.to_string());
                entry.flow_id = Some(native_correlation.flow_id.to_string());
                entry.task_id = Some(native_correlation.task_id.to_string());
                entry.attempt_id = Some(native_correlation.attempt_id.to_string());
                entry.adapter_name = Some(adapter_name.clone());
                entry.provider = Some(provider.clone());
                entry.model = Some(model.clone());
                entry.agent_mode.clone_from(agent_mode);
                entry.configured_max_turns = *configured_max_turns;
                entry.configured_timeout_budget_ms = *configured_timeout_budget_ms;
                entry.configured_token_budget = *configured_token_budget;
                entry.configured_prompt_headroom = *configured_prompt_headroom;
                entry.allowed_tools = dedup_strings(allowed_tools.iter().cloned());
                entry.allowed_capabilities = dedup_strings(allowed_capabilities.iter().cloned());
            }
            EventPayload::ToolCallFailed {
                invocation_id,
                turn_index,
                tool_name,
                code,
                policy_source,
                denial_reason,
                ..
            } if code == "native_tool_mode_denied" => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                entry.denied_tools.push(DeniedToolSummary {
                    turn_index: *turn_index,
                    tool_name: tool_name.clone(),
                    code: code.clone(),
                    policy_source: policy_source.clone(),
                    denial_reason: denial_reason.clone(),
                });
            }
            EventPayload::NativeBudgetThresholdReached {
                invocation_id,
                turn_index,
                threshold_percent,
                used_budget,
                total_budget,
                remaining_budget,
                ..
            } => {
                invocations
                    .entry(invocation_id.clone())
                    .or_default()
                    .budget_warnings
                    .push(BudgetWarningSummary {
                        turn_index: *turn_index,
                        threshold_percent: *threshold_percent,
                        used_budget: *used_budget,
                        total_budget: *total_budget,
                        remaining_budget: *remaining_budget,
                    });
            }
            EventPayload::NativeHistoryCompactionRecorded {
                invocation_id,
                turn_index,
                reason,
                rendered_prompt_bytes_before,
                selected_history_count_before,
                selected_history_chars_before,
                visible_items_before,
                visible_items_after,
                prompt_tokens_before,
                projected_budget_used,
                token_budget,
                elapsed_since_invocation_ms,
                ..
            } => {
                invocations
                    .entry(invocation_id.clone())
                    .or_default()
                    .history_compactions
                    .push(NativeHistoryCompactionRow {
                        turn_index: *turn_index,
                        reason: reason.clone(),
                        rendered_prompt_bytes_before: *rendered_prompt_bytes_before,
                        selected_history_count_before: *selected_history_count_before,
                        selected_history_chars_before: *selected_history_chars_before,
                        visible_items_before: *visible_items_before,
                        visible_items_after: *visible_items_after,
                        prompt_tokens_before: *prompt_tokens_before,
                        projected_budget_used: *projected_budget_used,
                        token_budget: *token_budget,
                        elapsed_since_invocation_ms: *elapsed_since_invocation_ms,
                    });
            }
            EventPayload::NativeTurnSummaryRecorded {
                invocation_id,
                turn_index,
                agent_mode,
                from_state,
                to_state,
                prompt_hash,
                context_manifest_hash,
                delivered_context_hash,
                mode_contract_hash,
                inputs_hash,
                prompt_headroom,
                available_budget,
                rendered_prompt_bytes,
                runtime_context_bytes,
                static_prompt_chars,
                visible_item_count,
                selected_history_count,
                selected_history_chars,
                code_navigation_count,
                code_navigation_chars,
                compacted_summary_count,
                compacted_summary_chars,
                tool_contract_count,
                tool_contract_chars,
                assembly_duration_ms,
                skipped_item_count,
                truncated_item_count,
                tool_result_items_visible,
                latest_tool_result_turn_index,
                latest_tool_names_visible,
                tool_call_count,
                tool_failure_count,
                model_latency_ms,
                tool_latency_ms,
                turn_duration_ms,
                elapsed_since_invocation_ms,
                request_tokens,
                response_tokens,
                budget_used_before,
                budget_used_after,
                budget_remaining,
                budget_thresholds_crossed,
                summary,
                ..
            } => {
                let row = NativeTurnSummaryRow {
                    turn_index: *turn_index,
                    agent_mode: agent_mode.clone(),
                    from_state: Some(from_state.clone()),
                    to_state: Some(to_state.clone()),
                    prompt_hash: prompt_hash.clone(),
                    context_manifest_hash: context_manifest_hash.clone(),
                    delivered_context_hash: delivered_context_hash.clone(),
                    mode_contract_hash: mode_contract_hash.clone(),
                    inputs_hash: inputs_hash.clone(),
                    prompt_headroom: *prompt_headroom,
                    available_budget: *available_budget,
                    rendered_prompt_bytes: *rendered_prompt_bytes,
                    runtime_context_bytes: *runtime_context_bytes,
                    static_prompt_chars: *static_prompt_chars,
                    visible_item_count: *visible_item_count,
                    selected_history_count: *selected_history_count,
                    selected_history_chars: *selected_history_chars,
                    code_navigation_count: *code_navigation_count,
                    code_navigation_chars: *code_navigation_chars,
                    compacted_summary_count: *compacted_summary_count,
                    compacted_summary_chars: *compacted_summary_chars,
                    tool_contract_count: *tool_contract_count,
                    tool_contract_chars: *tool_contract_chars,
                    assembly_duration_ms: *assembly_duration_ms,
                    skipped_item_count: *skipped_item_count,
                    truncated_item_count: *truncated_item_count,
                    tool_result_items_visible: *tool_result_items_visible,
                    latest_tool_result_turn_index: *latest_tool_result_turn_index,
                    latest_tool_names_visible: latest_tool_names_visible.clone(),
                    tool_call_count: *tool_call_count,
                    tool_failure_count: *tool_failure_count,
                    budget_used_before: *budget_used_before,
                    budget_used_after: *budget_used_after,
                    budget_remaining: *budget_remaining,
                    budget_thresholds_crossed: budget_thresholds_crossed.clone(),
                    model_latency_ms: *model_latency_ms,
                    tool_latency_ms: *tool_latency_ms,
                    turn_duration_ms: *turn_duration_ms,
                    elapsed_since_invocation_ms: *elapsed_since_invocation_ms,
                    request_tokens: *request_tokens,
                    response_tokens: *response_tokens,
                    summary: summary.clone(),
                };
                invocations
                    .entry(invocation_id.clone())
                    .or_default()
                    .turns
                    .push(row);
            }
            EventPayload::AgentInvocationCompleted {
                invocation_id,
                total_turns,
                final_state,
                success,
                error_code,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                entry.total_turns = *total_turns;
                entry.final_state = Some(final_state.clone());
                entry.success = Some(*success);
                if let Some(code) = error_code.clone() {
                    push_unique(&mut entry.failure_codes, code);
                }
            }
            _ => {}
        }
    }

    let mut summaries = invocations.into_values().collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.invocation_id.cmp(&right.invocation_id));
    for summary in &mut summaries {
        summary.total_model_latency_ms =
            summary.turns.iter().map(|turn| turn.model_latency_ms).sum();
        summary.total_tool_latency_ms = summary.turns.iter().map(|turn| turn.tool_latency_ms).sum();
        summary.total_turn_duration_ms =
            summary.turns.iter().map(|turn| turn.turn_duration_ms).sum();
        summary.total_request_tokens = summary.turns.iter().map(|turn| turn.request_tokens).sum();
        summary.total_response_tokens = summary.turns.iter().map(|turn| turn.response_tokens).sum();
        summary.max_rendered_prompt_bytes = summary
            .turns
            .iter()
            .map(|turn| turn.rendered_prompt_bytes)
            .max()
            .unwrap_or_default();
        summary.max_selected_history_count = summary
            .turns
            .iter()
            .map(|turn| turn.selected_history_count)
            .max()
            .unwrap_or_default();
        summary.max_selected_history_chars = summary
            .turns
            .iter()
            .map(|turn| turn.selected_history_chars)
            .max()
            .unwrap_or_default();
        summary.max_assembly_duration_ms = summary
            .turns
            .iter()
            .map(|turn| turn.assembly_duration_ms)
            .max()
            .unwrap_or_default();
        summary.max_elapsed_since_invocation_ms = summary
            .turns
            .iter()
            .map(|turn| turn.elapsed_since_invocation_ms)
            .max()
            .unwrap_or_default();
        summary.turns.sort_by_key(|turn| turn.turn_index);
        summary
            .history_compactions
            .sort_by_key(|row| (row.turn_index, row.elapsed_since_invocation_ms));
        verify_summary(summary, &mut verification.failures);
    }
    verification.passed = verification.failures.is_empty();
    NativeSummaryReport {
        invocation_count: summaries.len(),
        invocations: summaries,
        verification,
    }
}

fn verify_summary(summary: &NativeInvocationSummary, failures: &mut Vec<String>) {
    if summary.total_turns > 0
        && usize::try_from(summary.total_turns).ok() != Some(summary.turns.len())
    {
        failures.push(format!(
            "invocation {} turn count mismatch: expected {}, saw {} summaries",
            summary.invocation_id,
            summary.total_turns,
            summary.turns.len()
        ));
    }
    if summary.total_turns > 0 && summary.success.is_none() {
        failures.push(format!(
            "invocation {} missing completion event",
            summary.invocation_id
        ));
    }
    let mut previous_had_tool_calls = false;
    for turn in &summary.turns {
        if turn.prompt_hash.is_none()
            || turn.context_manifest_hash.is_none()
            || turn.delivered_context_hash.is_none()
        {
            failures.push(format!(
                "invocation {} turn {} missing prompt/context hashes",
                summary.invocation_id, turn.turn_index
            ));
        }
        if turn.mode_contract_hash.is_none() || turn.inputs_hash.is_none() {
            failures.push(format!(
                "invocation {} turn {} missing mode/input hashes",
                summary.invocation_id, turn.turn_index
            ));
        }
        if previous_had_tool_calls
            && turn.tool_result_items_visible == 0
            && turn.code_navigation_count == 0
            && turn.latest_tool_result_turn_index.is_none()
        {
            failures.push(format!(
                "invocation {} turn {} did not expose prior tool results",
                summary.invocation_id, turn.turn_index
            ));
        }
        previous_had_tool_calls = turn.tool_call_count > 0;
    }
}

fn dedup_strings(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let set = values.into_iter().collect::<BTreeSet<_>>();
    set.into_iter().collect()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
        values.sort();
    }
}
