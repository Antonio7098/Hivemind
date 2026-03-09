use crate::cli::commands::EventNativeSummaryArgs;
use crate::cli::handlers::common::print_structured;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::events::{Event, EventPayload};
use crate::core::registry::Registry;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

use super::filter::build_event_filter;

#[derive(Debug, Default, Serialize)]
pub(super) struct NativeSummaryReport {
    pub invocation_count: usize,
    pub invocations: Vec<NativeInvocationSummary>,
    pub verification: NativeVerificationSummary,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct NativeVerificationSummary {
    pub passed: bool,
    pub failures: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct NativeInvocationSummary {
    pub invocation_id: String,
    pub project_id: Option<String>,
    pub graph_id: Option<String>,
    pub flow_id: Option<String>,
    pub task_id: Option<String>,
    pub attempt_id: Option<String>,
    pub adapter_name: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub agent_mode: Option<String>,
    pub configured_max_turns: Option<u32>,
    pub configured_timeout_budget_ms: Option<u64>,
    pub configured_token_budget: Option<usize>,
    pub configured_prompt_headroom: Option<usize>,
    pub allowed_tools: Vec<String>,
    pub allowed_capabilities: Vec<String>,
    pub total_turns: u32,
    pub success: Option<bool>,
    pub final_state: Option<String>,
    pub failure_codes: Vec<String>,
    pub denied_tools: Vec<DeniedToolSummary>,
    pub budget_warnings: Vec<BudgetWarningSummary>,
    pub history_compactions: Vec<NativeHistoryCompactionRow>,
    pub total_model_latency_ms: u64,
    pub total_tool_latency_ms: u64,
    pub total_turn_duration_ms: u64,
    pub total_request_tokens: usize,
    pub total_response_tokens: usize,
    pub max_rendered_prompt_bytes: usize,
    pub max_selected_history_count: usize,
    pub max_selected_history_chars: usize,
    pub max_assembly_duration_ms: u64,
    pub max_elapsed_since_invocation_ms: u64,
    pub turns: Vec<NativeTurnSummaryRow>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct DeniedToolSummary {
    pub turn_index: u32,
    pub tool_name: String,
    pub code: String,
    pub policy_source: Option<String>,
    pub denial_reason: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct BudgetWarningSummary {
    pub turn_index: u32,
    pub threshold_percent: u8,
    pub used_budget: usize,
    pub total_budget: usize,
    pub remaining_budget: usize,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct NativeHistoryCompactionRow {
    pub turn_index: u32,
    pub reason: String,
    pub rendered_prompt_bytes_before: usize,
    pub selected_history_count_before: usize,
    pub selected_history_chars_before: usize,
    pub visible_items_before: usize,
    pub visible_items_after: usize,
    pub prompt_tokens_before: usize,
    pub projected_budget_used: usize,
    pub token_budget: usize,
    pub elapsed_since_invocation_ms: u64,
}

#[derive(Debug, Default, Serialize, Clone)]
pub(super) struct NativeTurnSummaryRow {
    pub turn_index: u32,
    pub agent_mode: Option<String>,
    pub from_state: Option<String>,
    pub to_state: Option<String>,
    pub prompt_hash: Option<String>,
    pub context_manifest_hash: Option<String>,
    pub delivered_context_hash: Option<String>,
    pub mode_contract_hash: Option<String>,
    pub inputs_hash: Option<String>,
    pub prompt_headroom: Option<usize>,
    pub available_budget: usize,
    pub rendered_prompt_bytes: usize,
    pub runtime_context_bytes: usize,
    pub static_prompt_chars: usize,
    pub visible_item_count: usize,
    pub selected_history_count: usize,
    pub selected_history_chars: usize,
    pub code_navigation_count: usize,
    pub code_navigation_chars: usize,
    pub compacted_summary_count: usize,
    pub compacted_summary_chars: usize,
    pub tool_contract_count: usize,
    pub tool_contract_chars: usize,
    pub assembly_duration_ms: u64,
    pub skipped_item_count: usize,
    pub truncated_item_count: usize,
    pub tool_result_items_visible: usize,
    pub latest_tool_result_turn_index: Option<u32>,
    pub latest_tool_names_visible: Vec<String>,
    pub tool_call_count: usize,
    pub tool_failure_count: usize,
    pub budget_used_before: usize,
    pub budget_used_after: usize,
    pub budget_remaining: usize,
    pub budget_thresholds_crossed: Vec<u8>,
    pub model_latency_ms: u64,
    pub tool_latency_ms: u64,
    pub turn_duration_ms: u64,
    pub elapsed_since_invocation_ms: u64,
    pub request_tokens: usize,
    pub response_tokens: usize,
    pub summary: Option<String>,
}

pub(super) fn handle_events_native_summary(
    registry: &Registry,
    args: &EventNativeSummaryArgs,
    format: OutputFormat,
) -> ExitCode {
    let filter = match build_event_filter(
        registry,
        "cli:events:native-summary",
        args.project.as_deref(),
        args.graph.as_deref(),
        args.flow.as_deref(),
        args.task.as_deref(),
        args.attempt.as_deref(),
        args.artifact_id.as_deref(),
        args.template_id.as_deref(),
        args.rule_id.as_deref(),
        args.error_type.as_deref(),
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    ) {
        Ok(filter) => filter,
        Err(error) => return output_error(&error, format),
    };

    let events = match registry.read_events(&filter) {
        Ok(events) => events,
        Err(error) => return output_error(&error, format),
    };

    let report = build_native_summary(&events);
    let passed = report.verification.failures.is_empty();
    print_structured(&report, format, "native event summary");
    if args.verify && !passed {
        ExitCode::Error
    } else {
        ExitCode::Success
    }
}

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

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    use super::*;
    use crate::core::events::{
        CorrelationIds, Event, NativeEventCorrelation, NativeEventPayloadCaptureMode,
    };
    use uuid::Uuid;

    #[test]
    fn native_summary_reports_passed_verification() {
        let flow = NativeEventCorrelation {
            project_id: Uuid::new_v4(),
            graph_id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
        };
        let correlation = CorrelationIds::for_graph(flow.project_id, flow.graph_id);
        let events = vec![
            Event::new(
                EventPayload::AgentInvocationStarted {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-1".to_string(),
                    adapter_name: "native".to_string(),
                    provider: "test".to_string(),
                    model: "mock".to_string(),
                    runtime_version: "1".to_string(),
                    capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                    agent_mode: Some("planner".to_string()),
                    allowed_tools: vec!["read_file".to_string()],
                    allowed_capabilities: vec!["read".to_string()],
                    configured_max_turns: Some(8),
                    configured_timeout_budget_ms: Some(300_000),
                    configured_token_budget: Some(32_000),
                    configured_prompt_headroom: Some(4_096),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallFailed {
                    native_correlation: flow.clone(),
                    task_id: None,
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    call_id: "call-1".to_string(),
                    tool_name: "read_file".to_string(),
                    code: "native_tool_input_invalid".to_string(),
                    message: "bad input".to_string(),
                    recoverable: true,
                    duration_ms: Some(1),
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: Some("retry with valid schema".to_string()),
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::NativeTurnSummaryRecorded {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    agent_mode: Some("planner".to_string()),
                    from_state: "think".to_string(),
                    to_state: "done".to_string(),
                    prompt_hash: Some("prompt".to_string()),
                    context_manifest_hash: Some("manifest".to_string()),
                    delivered_context_hash: Some("context".to_string()),
                    mode_contract_hash: Some("mode".to_string()),
                    inputs_hash: Some("inputs".to_string()),
                    prompt_headroom: Some(128),
                    available_budget: 1024,
                    rendered_prompt_bytes: 512,
                    runtime_context_bytes: 64,
                    static_prompt_chars: 128,
                    visible_item_count: 3,
                    selected_history_count: 1,
                    selected_history_chars: 48,
                    code_navigation_count: 1,
                    code_navigation_chars: 24,
                    compacted_summary_count: 0,
                    compacted_summary_chars: 0,
                    tool_contract_count: 1,
                    tool_contract_chars: 32,
                    assembly_duration_ms: 1,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: Vec::new(),
                    tool_call_count: 0,
                    tool_failure_count: 0,
                    model_latency_ms: 1,
                    tool_latency_ms: 0,
                    turn_duration_ms: 1,
                    elapsed_since_invocation_ms: 1,
                    request_tokens: 10,
                    response_tokens: 5,
                    budget_used_before: 0,
                    budget_used_after: 10,
                    budget_remaining: 1014,
                    budget_thresholds_crossed: Vec::new(),
                    summary: Some("done".to_string()),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::AgentInvocationCompleted {
                    native_correlation: flow,
                    invocation_id: "inv-1".to_string(),
                    total_turns: 1,
                    final_state: "done".to_string(),
                    success: true,
                    final_summary: Some("done".to_string()),
                    error_code: None,
                    error_message: None,
                    recoverable: None,
                },
                correlation,
            ),
        ];

        let summary = build_native_summary(&events);
        assert_eq!(summary.invocation_count, 1);
        assert!(
            summary.verification.passed,
            "{:?}",
            summary.verification.failures
        );
        assert_eq!(summary.invocations[0].success, Some(true));
        assert!(summary.invocations[0].failure_codes.is_empty());
        assert_eq!(summary.invocations[0].configured_max_turns, Some(8));
        assert_eq!(
            summary.invocations[0].turns[0]
                .mode_contract_hash
                .as_deref(),
            Some("mode")
        );
    }

    #[test]
    fn native_summary_accepts_prior_tool_results_delivered_via_navigation() {
        let flow = NativeEventCorrelation {
            project_id: Uuid::new_v4(),
            graph_id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
        };
        let correlation = CorrelationIds {
            project_id: Some(flow.project_id),
            graph_id: Some(flow.graph_id),
            flow_id: Some(flow.flow_id),
            task_id: Some(flow.task_id),
            attempt_id: Some(flow.attempt_id),
        };
        let events = vec![
            Event::new(
                EventPayload::NativeTurnSummaryRecorded {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-2".to_string(),
                    turn_index: 0,
                    from_state: "think".to_string(),
                    to_state: "act".to_string(),
                    agent_mode: Some("freeflow".to_string()),
                    prompt_hash: Some("p0".to_string()),
                    context_manifest_hash: Some("m0".to_string()),
                    delivered_context_hash: Some("d0".to_string()),
                    prompt_headroom: Some(768),
                    available_budget: 1000,
                    mode_contract_hash: Some("mode".to_string()),
                    inputs_hash: Some("inputs".to_string()),
                    rendered_prompt_bytes: 100,
                    runtime_context_bytes: 50,
                    static_prompt_chars: 80,
                    visible_item_count: 2,
                    selected_history_count: 2,
                    selected_history_chars: 40,
                    code_navigation_count: 0,
                    code_navigation_chars: 0,
                    compacted_summary_count: 0,
                    compacted_summary_chars: 0,
                    tool_contract_count: 1,
                    tool_contract_chars: 20,
                    assembly_duration_ms: 1,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: Vec::new(),
                    tool_call_count: 1,
                    tool_failure_count: 0,
                    model_latency_ms: 1,
                    tool_latency_ms: 1,
                    turn_duration_ms: 2,
                    elapsed_since_invocation_ms: 2,
                    request_tokens: 8,
                    response_tokens: 4,
                    budget_used_before: 0,
                    budget_used_after: 10,
                    budget_remaining: 990,
                    budget_thresholds_crossed: Vec::new(),
                    summary: None,
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::NativeTurnSummaryRecorded {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-2".to_string(),
                    turn_index: 1,
                    from_state: "act".to_string(),
                    to_state: "done".to_string(),
                    agent_mode: Some("freeflow".to_string()),
                    prompt_hash: Some("p1".to_string()),
                    context_manifest_hash: Some("m1".to_string()),
                    delivered_context_hash: Some("d1".to_string()),
                    prompt_headroom: Some(768),
                    available_budget: 980,
                    mode_contract_hash: Some("mode".to_string()),
                    inputs_hash: Some("inputs".to_string()),
                    rendered_prompt_bytes: 120,
                    runtime_context_bytes: 60,
                    static_prompt_chars: 90,
                    visible_item_count: 4,
                    selected_history_count: 3,
                    selected_history_chars: 60,
                    code_navigation_count: 1,
                    code_navigation_chars: 24,
                    compacted_summary_count: 0,
                    compacted_summary_chars: 0,
                    tool_contract_count: 1,
                    tool_contract_chars: 20,
                    assembly_duration_ms: 1,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: Vec::new(),
                    tool_call_count: 0,
                    tool_failure_count: 0,
                    model_latency_ms: 1,
                    tool_latency_ms: 0,
                    turn_duration_ms: 1,
                    elapsed_since_invocation_ms: 3,
                    request_tokens: 9,
                    response_tokens: 4,
                    budget_used_before: 10,
                    budget_used_after: 20,
                    budget_remaining: 980,
                    budget_thresholds_crossed: Vec::new(),
                    summary: Some("done".to_string()),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::AgentInvocationCompleted {
                    native_correlation: flow,
                    invocation_id: "inv-2".to_string(),
                    total_turns: 2,
                    final_state: "done".to_string(),
                    success: true,
                    final_summary: Some("done".to_string()),
                    error_code: None,
                    error_message: None,
                    recoverable: None,
                },
                correlation,
            ),
        ];

        let summary = build_native_summary(&events);
        assert!(
            summary.verification.passed,
            "{:?}",
            summary.verification.failures
        );
    }
}
