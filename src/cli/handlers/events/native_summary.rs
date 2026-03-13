use crate::cli::commands::EventNativeSummaryArgs;
use crate::cli::handlers::common::print_structured;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::events::{Event, EventPayload, NativeBlobRef};
use crate::core::registry::Registry;
use crate::native::ModelTurnRequest;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

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
    pub runtime_started_at: Option<String>,
    pub invocation_started_at: Option<String>,
    pub first_model_request_prepared_at: Option<String>,
    pub first_tool_call_requested_at: Option<String>,
    pub runtime_to_invocation_ms: Option<i64>,
    pub invocation_to_first_model_request_ms: Option<i64>,
    pub invocation_to_first_tool_call_ms: Option<i64>,
    pub runtime_output_chunk_count: usize,
    pub startup_progress: Vec<NativeStartupProgressRow>,
    pub allowed_tools: Vec<String>,
    pub allowed_capabilities: Vec<String>,
    pub total_turns: u32,
    pub success: Option<bool>,
    pub final_state: Option<String>,
    pub final_summary: Option<String>,
    pub failure_codes: Vec<String>,
    pub denied_tools: Vec<DeniedToolSummary>,
    pub nonrecoverable_tool_failures: Vec<NativeToolFailureSummary>,
    pub validation_attempts: Vec<NativeValidationAttemptRow>,
    pub whole_file_overwrites: Vec<NativeWholeFileOverwriteRow>,
    pub suspicious_signals: Vec<String>,
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

#[derive(Debug, Default, Serialize, Clone)]
pub(super) struct NativeToolFailureSummary {
    pub turn_index: u32,
    pub tool_name: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize, Clone)]
pub(super) struct NativeValidationAttemptRow {
    pub turn_index: u32,
    pub tool_name: String,
    pub command_line: String,
    pub outcome: String,
    pub exit_code: Option<i64>,
    pub message: Option<String>,
}

#[derive(Debug, Default, Serialize, Clone)]
pub(super) struct NativeWholeFileOverwriteRow {
    pub turn_index: u32,
    pub path: String,
    pub content_bytes: usize,
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
pub(super) struct NativeStartupProgressRow {
    pub timestamp: String,
    pub stage: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub line: String,
}

#[derive(Debug, Default, Clone)]
struct PendingAttemptStartup {
    runtime_started_at: Option<DateTime<Utc>>,
    runtime_output_chunk_count: usize,
    startup_progress: Vec<NativeStartupProgressRow>,
}

#[derive(Debug, Clone)]
struct PendingToolRequest {
    command_line: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconstructed_context_window: Option<NativeTurnContextWindow>,
}

#[derive(Debug, Serialize, Clone)]
pub(super) struct NativeTurnContextWindow {
    pub digest: String,
    pub blob_path: String,
    pub request: ModelTurnRequest,
}

#[derive(Debug, Default, Clone, Copy)]
struct NativeSummaryBuildOptions {
    include_reconstructed_context: bool,
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

    let report = if args.include_reconstructed_context {
        build_native_summary_with_options(
            &events,
            NativeSummaryBuildOptions {
                include_reconstructed_context: true,
            },
        )
    } else {
        build_native_summary(&events)
    };
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
    build_native_summary_with_options(events, NativeSummaryBuildOptions::default())
}

#[allow(clippy::too_many_lines)]
fn build_native_summary_with_options(
    events: &[Event],
    options: NativeSummaryBuildOptions,
) -> NativeSummaryReport {
    let mut invocations = BTreeMap::<String, NativeInvocationSummary>::new();
    let mut pending_by_attempt = BTreeMap::<String, PendingAttemptStartup>::new();
    let mut attempt_to_invocation = BTreeMap::<String, String>::new();
    let mut invocation_started_at = BTreeMap::<String, DateTime<Utc>>::new();
    let mut pending_tool_requests = BTreeMap::<String, PendingToolRequest>::new();
    let mut reconstructed_context_by_turn =
        BTreeMap::<(String, u32), NativeTurnContextWindow>::new();
    let mut verification = NativeVerificationSummary::default();
    for event in events {
        match &event.payload {
            EventPayload::RuntimeStarted { attempt_id, .. } => {
                pending_by_attempt
                    .entry(attempt_id.to_string())
                    .or_default()
                    .runtime_started_at = Some(event.timestamp());
            }
            EventPayload::RuntimeOutputChunk {
                attempt_id,
                content,
                ..
            } => {
                let pending = pending_by_attempt
                    .entry(attempt_id.to_string())
                    .or_default();
                pending.runtime_output_chunk_count += 1;
                if let Some(progress) = parse_native_progress_row(event.timestamp(), content) {
                    push_startup_progress(&mut pending.startup_progress, progress.clone());
                }
                if let Some(invocation_id) = attempt_to_invocation.get(&attempt_id.to_string()) {
                    let entry = invocations.entry(invocation_id.clone()).or_default();
                    entry.runtime_output_chunk_count += 1;
                    if let Some(progress) = parse_native_progress_row(event.timestamp(), content) {
                        push_startup_progress(&mut entry.startup_progress, progress);
                    }
                }
            }
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
                let attempt_id = native_correlation.attempt_id.to_string();
                let started_at = event.timestamp();
                attempt_to_invocation.insert(attempt_id.clone(), invocation_id.clone());
                invocation_started_at.insert(invocation_id.clone(), started_at);
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
                let invocation_started = invocation_started_at
                    .get(invocation_id)
                    .cloned()
                    .unwrap_or_else(|| event.timestamp());
                entry.invocation_started_at = Some(invocation_started.to_rfc3339());
                entry.allowed_tools = dedup_strings(allowed_tools.iter().cloned());
                entry.allowed_capabilities = dedup_strings(allowed_capabilities.iter().cloned());
                if let Some(pending) = pending_by_attempt.get(&attempt_id) {
                    entry.runtime_output_chunk_count = pending.runtime_output_chunk_count;
                    entry.startup_progress = pending.startup_progress.clone();
                    if let Some(runtime_started_at) = pending.runtime_started_at.as_ref() {
                        entry.runtime_started_at = Some(runtime_started_at.to_rfc3339());
                        entry.runtime_to_invocation_ms = Some(
                            invocation_started
                                .signed_duration_since(runtime_started_at.to_owned())
                                .num_milliseconds(),
                        );
                    }
                }
            }
            EventPayload::ModelRequestPrepared {
                invocation_id,
                turn_index,
                request,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                if entry.first_model_request_prepared_at.is_none() {
                    let timestamp = event.timestamp();
                    entry.first_model_request_prepared_at = Some(timestamp.to_rfc3339());
                    if let Some(started_at) = invocation_started_at.get(invocation_id) {
                        entry.invocation_to_first_model_request_ms =
                            Some((timestamp - *started_at).num_milliseconds());
                    }
                }
                if options.include_reconstructed_context {
                    if let Some(window) = reconstruct_context_window(request) {
                        reconstructed_context_by_turn
                            .insert((invocation_id.clone(), *turn_index), window);
                    }
                }
            }
            EventPayload::ToolCallRequested {
                invocation_id,
                turn_index,
                call_id,
                tool_name,
                request,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                if entry.first_tool_call_requested_at.is_none() {
                    let timestamp = event.timestamp();
                    entry.first_tool_call_requested_at = Some(timestamp.to_rfc3339());
                    if let Some(started_at) = invocation_started_at.get(invocation_id) {
                        entry.invocation_to_first_tool_call_ms =
                            Some((timestamp - *started_at).num_milliseconds());
                    }
                }
                if let Some(command_line) = tool_request_command_line(tool_name, request) {
                    pending_tool_requests
                        .insert(call_id.clone(), PendingToolRequest { command_line });
                }
                if let Some(overwrite) = parse_whole_file_overwrite(*turn_index, tool_name, request)
                {
                    entry.whole_file_overwrites.push(overwrite);
                }
            }
            EventPayload::ToolCallFailed {
                invocation_id,
                turn_index,
                call_id,
                tool_name,
                code,
                message,
                recoverable,
                policy_source,
                denial_reason,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                if code == "native_tool_mode_denied" {
                    entry.denied_tools.push(DeniedToolSummary {
                        turn_index: *turn_index,
                        tool_name: tool_name.clone(),
                        code: code.clone(),
                        policy_source: policy_source.clone(),
                        denial_reason: denial_reason.clone(),
                    });
                }
                if !recoverable {
                    entry
                        .nonrecoverable_tool_failures
                        .push(NativeToolFailureSummary {
                            turn_index: *turn_index,
                            tool_name: tool_name.clone(),
                            code: code.clone(),
                            message: message.clone(),
                        });
                }
                if let Some(pending) = pending_tool_requests.remove(call_id) {
                    entry.validation_attempts.push(NativeValidationAttemptRow {
                        turn_index: *turn_index,
                        tool_name: tool_name.clone(),
                        command_line: pending.command_line,
                        outcome: "tool_failed".to_string(),
                        exit_code: None,
                        message: Some(message.clone()),
                    });
                }
            }
            EventPayload::ToolCallCompleted {
                invocation_id,
                turn_index,
                call_id,
                tool_name,
                response,
                ..
            } => {
                if let Some(pending) = pending_tool_requests.remove(call_id) {
                    let exit_code = parse_tool_response_exit_code(response);
                    let message = exit_code.map(|code| format!("exit_code={code}"));
                    invocations
                        .entry(invocation_id.clone())
                        .or_default()
                        .validation_attempts
                        .push(NativeValidationAttemptRow {
                            turn_index: *turn_index,
                            tool_name: tool_name.clone(),
                            command_line: pending.command_line,
                            outcome: if exit_code == Some(0) {
                                "passed".to_string()
                            } else {
                                "nonzero_exit".to_string()
                            },
                            exit_code,
                            message,
                        });
                }
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
                    reconstructed_context_window: reconstructed_context_by_turn
                        .get(&(invocation_id.clone(), *turn_index))
                        .cloned(),
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
                final_summary,
                error_code,
                ..
            } => {
                let entry = invocations.entry(invocation_id.clone()).or_default();
                entry.total_turns = *total_turns;
                entry.final_state = Some(final_state.clone());
                entry.final_summary.clone_from(final_summary);
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
            .nonrecoverable_tool_failures
            .sort_by_key(|row| (row.turn_index, row.tool_name.clone()));
        summary
            .validation_attempts
            .sort_by_key(|row| (row.turn_index, row.command_line.clone()));
        summary
            .whole_file_overwrites
            .sort_by_key(|row| (row.turn_index, row.path.clone()));
        summary
            .history_compactions
            .sort_by_key(|row| (row.turn_index, row.elapsed_since_invocation_ms));
        populate_suspicious_signals(summary);
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
    if summary.success == Some(true) && !summary.nonrecoverable_tool_failures.is_empty() {
        failures.push(format!(
            "invocation {} reported success despite {} non-recoverable tool failures",
            summary.invocation_id,
            summary.nonrecoverable_tool_failures.len()
        ));
    }
    let successful_validation = summary
        .validation_attempts
        .iter()
        .any(|attempt| attempt.outcome == "passed");
    if summary.success == Some(true)
        && summary_claims_validation(summary)
        && !summary.validation_attempts.is_empty()
        && !successful_validation
    {
        failures.push(format!(
            "invocation {} claimed validation success without any passing validation command",
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

fn read_native_blob_text(blob: &NativeBlobRef) -> Option<String> {
    blob.payload
        .clone()
        .or_else(|| fs::read_to_string(&blob.blob_path).ok())
}

fn parse_tool_request_payload(request: &NativeBlobRef) -> Option<Value> {
    serde_json::from_str(&read_native_blob_text(request)?).ok()
}

fn reconstruct_context_window(request: &NativeBlobRef) -> Option<NativeTurnContextWindow> {
    let payload = read_native_blob_text(request)?;
    let parsed = serde_json::from_str::<ModelTurnRequest>(&payload).ok()?;
    Some(NativeTurnContextWindow {
        digest: request.digest.clone(),
        blob_path: request.blob_path.clone(),
        request: parsed,
    })
}

fn tool_request_command_line(tool_name: &str, request: &NativeBlobRef) -> Option<String> {
    let payload = parse_tool_request_payload(request)?;
    let input = payload.get("input")?.as_object()?;
    let primary = match tool_name {
        "run_command" => "command",
        "exec_command" => "cmd",
        _ => return None,
    };
    let mut command = input.get(primary)?.as_str()?.trim().to_string();
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
        let mut parts = command.split_whitespace();
        let split_command = parts.next()?.to_string();
        let split_args = parts.map(ToString::to_string).collect::<Vec<_>>();
        if !split_args.is_empty() {
            command = split_command;
            args = split_args;
        }
    }
    if !is_validation_command(&command, &args) {
        return None;
    }
    if args.is_empty() {
        Some(command)
    } else {
        Some(format!("{} {}", command, args.join(" ")))
    }
}

fn parse_whole_file_overwrite(
    turn_index: u32,
    tool_name: &str,
    request: &NativeBlobRef,
) -> Option<NativeWholeFileOverwriteRow> {
    if tool_name != "write_file" {
        return None;
    }
    let payload = parse_tool_request_payload(request)?;
    let input = payload.get("input")?.as_object()?;
    if input
        .get("append")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    Some(NativeWholeFileOverwriteRow {
        turn_index,
        path: input.get("path")?.as_str()?.to_string(),
        content_bytes: input.get("content")?.as_str()?.len(),
    })
}

fn parse_tool_response_exit_code(response: &NativeBlobRef) -> Option<i64> {
    let payload: Value = serde_json::from_str(&read_native_blob_text(response)?).ok()?;
    payload.get("output")?.get("exit_code")?.as_i64()
}

fn is_validation_command(command: &str, args: &[String]) -> bool {
    let lowered_command = command.trim().to_ascii_lowercase();
    let first_arg = args.first().map(|arg| arg.trim().to_ascii_lowercase());
    matches!(
        (lowered_command.as_str(), first_arg.as_deref()),
        ("cargo", Some("test" | "check" | "clippy" | "fmt"))
            | (
                "go" | "just" | "npm" | "pnpm" | "yarn" | "bun",
                Some("test")
            )
            | ("pytest" | "py.test", _)
    )
}

fn summary_claims_validation(summary: &NativeInvocationSummary) -> bool {
    let Some(text) = summary
        .final_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let lowered = text.to_ascii_lowercase();
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

fn populate_suspicious_signals(summary: &mut NativeInvocationSummary) {
    if summary.success == Some(true) && !summary.nonrecoverable_tool_failures.is_empty() {
        summary.suspicious_signals.push(format!(
            "reported success despite {} non-recoverable tool failures",
            summary.nonrecoverable_tool_failures.len()
        ));
    }
    let failed_validation_count = summary
        .validation_attempts
        .iter()
        .filter(|attempt| attempt.outcome != "passed")
        .count();
    let successful_validation_count = summary
        .validation_attempts
        .iter()
        .filter(|attempt| attempt.outcome == "passed")
        .count();
    if failed_validation_count > 0 {
        summary.suspicious_signals.push(format!(
            "observed {failed_validation_count} failed validation attempts"
        ));
    }
    if summary_claims_validation(summary)
        && !summary.validation_attempts.is_empty()
        && successful_validation_count == 0
    {
        summary.suspicious_signals.push(
            "final summary claimed validation success without any passing validation command"
                .to_string(),
        );
    }
    if summary.whole_file_overwrites.len() >= 2 {
        summary.suspicious_signals.push(format!(
            "observed {} whole-file overwrite writes via write_file append=false",
            summary.whole_file_overwrites.len()
        ));
    }
}

fn parse_native_progress_row(
    timestamp: DateTime<Utc>,
    content: &str,
) -> Option<NativeStartupProgressRow> {
    let line = content.trim();
    if !line.contains("[native-progress]") {
        return None;
    }
    Some(NativeStartupProgressRow {
        timestamp: timestamp.to_rfc3339(),
        stage: extract_progress_field(line, "stage="),
        elapsed_ms: extract_progress_field(line, "elapsed_ms=")
            .and_then(|value| value.parse().ok()),
        line: line.to_string(),
    })
}

fn extract_progress_field(line: &str, prefix: &str) -> Option<String> {
    line.split_whitespace()
        .find_map(|token| token.strip_prefix(prefix).map(ToOwned::to_owned))
}

fn push_startup_progress(rows: &mut Vec<NativeStartupProgressRow>, row: NativeStartupProgressRow) {
    if rows
        .last()
        .map(|existing| existing.line == row.line)
        .unwrap_or(false)
    {
        return;
    }
    if rows.len() < 32 {
        rows.push(row);
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)]
mod tests {
    use super::*;
    use crate::core::events::{
        CorrelationIds, Event, NativeBlobRef, NativeEventCorrelation,
        NativeEventPayloadCaptureMode, RuntimeOutputStream, RuntimeRole,
    };
    use uuid::Uuid;

    fn with_timestamp(mut event: Event, timestamp: &str) -> Event {
        event.metadata.timestamp = chrono::DateTime::parse_from_rfc3339(timestamp)
            .expect("timestamp")
            .with_timezone(&Utc);
        event
    }

    fn test_blob() -> NativeBlobRef {
        NativeBlobRef {
            digest: "digest".to_string(),
            byte_size: 4,
            media_type: "text/plain".to_string(),
            blob_path: "/tmp/blob".to_string(),
            payload: Some("test".to_string()),
        }
    }

    fn json_blob(value: Value) -> NativeBlobRef {
        let payload = serde_json::to_string(&value).expect("json payload");
        NativeBlobRef {
            digest: "digest-json".to_string(),
            byte_size: payload.len() as u64,
            media_type: "text/plain".to_string(),
            blob_path: "/tmp/blob.json".to_string(),
            payload: Some(payload),
        }
    }

    fn file_backed_json_blob(value: Value) -> NativeBlobRef {
        let payload = serde_json::to_string(&value).expect("json payload");
        let path = std::env::temp_dir().join(format!("native-summary-{}.json", Uuid::new_v4()));
        std::fs::write(&path, &payload).expect("write blob payload");
        NativeBlobRef {
            digest: "digest-file-json".to_string(),
            byte_size: payload.len() as u64,
            media_type: "application/json".to_string(),
            blob_path: path.to_string_lossy().to_string(),
            payload: None,
        }
    }

    #[test]
    fn native_summary_can_reconstruct_context_windows_from_blob_storage() {
        let flow = NativeEventCorrelation {
            project_id: Uuid::new_v4(),
            graph_id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
        };
        let correlation = CorrelationIds::for_graph(flow.project_id, flow.graph_id);
        let request_blob = file_backed_json_blob(serde_json::json!({
            "turn_index": 7,
            "state": "think",
            "agent_mode": "task_executor",
            "prompt": "reconstruct me",
            "context": "ctx",
            "prompt_assembly": null
        }));
        let events = vec![
            Event::new(
                EventPayload::AgentInvocationStarted {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-ctx".to_string(),
                    adapter_name: "native".to_string(),
                    provider: "openrouter".to_string(),
                    model: "nemotron".to_string(),
                    runtime_version: "1".to_string(),
                    capture_mode: NativeEventPayloadCaptureMode::FullPayload,
                    agent_mode: Some("task_executor".to_string()),
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
                EventPayload::ModelRequestPrepared {
                    native_correlation: flow.clone(),
                    invocation_id: "inv-ctx".to_string(),
                    turn_index: 7,
                    request: request_blob.clone(),
                    prompt_headroom: Some(128),
                    visible_item_count: 1,
                    available_budget: 900,
                    rendered_prompt_bytes: 14,
                    runtime_context_bytes: 3,
                    static_prompt_chars: 10,
                    selected_history_count: 0,
                    selected_history_chars: 0,
                    code_navigation_count: 0,
                    code_navigation_chars: 0,
                    compacted_summary_count: 0,
                    compacted_summary_chars: 0,
                    tool_contract_count: 0,
                    tool_contract_chars: 0,
                    assembly_duration_ms: 1,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: Vec::new(),
                    active_code_window_trace: Vec::new(),
                    delivery_target: Some("runtime_execution_input".to_string()),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::NativeTurnSummaryRecorded {
                    native_correlation: flow,
                    invocation_id: "inv-ctx".to_string(),
                    turn_index: 7,
                    agent_mode: Some("task_executor".to_string()),
                    from_state: "think".to_string(),
                    to_state: "act".to_string(),
                    prompt_hash: Some("prompt-7".to_string()),
                    context_manifest_hash: Some("manifest-7".to_string()),
                    delivered_context_hash: Some("context-7".to_string()),
                    mode_contract_hash: Some("mode-7".to_string()),
                    inputs_hash: Some("inputs-7".to_string()),
                    prompt_headroom: Some(128),
                    available_budget: 900,
                    rendered_prompt_bytes: 14,
                    runtime_context_bytes: 3,
                    static_prompt_chars: 10,
                    visible_item_count: 1,
                    selected_history_count: 0,
                    selected_history_chars: 0,
                    code_navigation_count: 0,
                    code_navigation_chars: 0,
                    compacted_summary_count: 0,
                    compacted_summary_chars: 0,
                    tool_contract_count: 0,
                    tool_contract_chars: 0,
                    assembly_duration_ms: 1,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: Vec::new(),
                    active_code_window_trace: Vec::new(),
                    tool_call_count: 0,
                    tool_failure_count: 0,
                    model_latency_ms: 1,
                    tool_latency_ms: 0,
                    turn_duration_ms: 1,
                    elapsed_since_invocation_ms: 1,
                    request_tokens: 3,
                    response_tokens: 2,
                    budget_used_before: 0,
                    budget_used_after: 3,
                    budget_remaining: 897,
                    budget_thresholds_crossed: Vec::new(),
                    summary: Some("turn 7".to_string()),
                },
                correlation,
            ),
        ];

        let report = build_native_summary_with_options(
            &events,
            NativeSummaryBuildOptions {
                include_reconstructed_context: true,
            },
        );

        let turn = &report.invocations[0].turns[0];
        let reconstructed = turn
            .reconstructed_context_window
            .as_ref()
            .expect("reconstructed context window");
        assert_eq!(reconstructed.digest, request_blob.digest);
        assert_eq!(reconstructed.request.turn_index, 7);
        assert_eq!(reconstructed.request.prompt, "reconstruct me");
        assert_eq!(reconstructed.request.context.as_deref(), Some("ctx"));
    }

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
                    active_code_window_trace: Vec::new(),
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
                    active_code_window_trace: Vec::new(),
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
                    active_code_window_trace: Vec::new(),
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

    #[test]
    fn native_summary_captures_startup_timing_and_progress() {
        let flow = NativeEventCorrelation {
            project_id: Uuid::new_v4(),
            graph_id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
        };
        let correlation = CorrelationIds::for_graph(flow.project_id, flow.graph_id);
        let events = vec![
            with_timestamp(
                Event::new(
                    EventPayload::RuntimeStarted {
                        adapter_name: "native".to_string(),
                        role: RuntimeRole::Worker,
                        task_id: flow.task_id,
                        attempt_id: flow.attempt_id,
                        prompt: String::new(),
                        flags: Vec::new(),
                    },
                    correlation.clone(),
                ),
                "2026-03-10T16:00:00Z",
            ),
            with_timestamp(
                Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id: flow.attempt_id,
                        stream: RuntimeOutputStream::Stdout,
                        content: "[native-progress] stage=invocation_starting elapsed_ms=12"
                            .to_string(),
                    },
                    correlation.clone(),
                ),
                "2026-03-10T16:00:00.012Z",
            ),
            with_timestamp(
                Event::new(
                    EventPayload::AgentInvocationStarted {
                        native_correlation: flow.clone(),
                        invocation_id: "inv-startup".to_string(),
                        adapter_name: "native".to_string(),
                        provider: "mock".to_string(),
                        model: "native-mock-v1".to_string(),
                        runtime_version: "1".to_string(),
                        capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                        agent_mode: Some("task_executor".to_string()),
                        allowed_tools: vec!["read_file".to_string()],
                        allowed_capabilities: vec!["read".to_string()],
                        configured_max_turns: Some(8),
                        configured_timeout_budget_ms: Some(30_000),
                        configured_token_budget: Some(10_000),
                        configured_prompt_headroom: Some(2_048),
                    },
                    correlation.clone(),
                ),
                "2026-03-10T16:00:00.050Z",
            ),
            with_timestamp(
                Event::new(
                    EventPayload::ModelRequestPrepared {
                        native_correlation: flow.clone(),
                        invocation_id: "inv-startup".to_string(),
                        turn_index: 0,
                        request: test_blob(),
                        prompt_headroom: Some(64),
                        visible_item_count: 1,
                        available_budget: 1_000,
                        rendered_prompt_bytes: 128,
                        runtime_context_bytes: 32,
                        static_prompt_chars: 64,
                        selected_history_count: 1,
                        selected_history_chars: 8,
                        code_navigation_count: 0,
                        code_navigation_chars: 0,
                        compacted_summary_count: 0,
                        compacted_summary_chars: 0,
                        tool_contract_count: 1,
                        tool_contract_chars: 10,
                        assembly_duration_ms: 1,
                        skipped_item_count: 0,
                        truncated_item_count: 0,
                        tool_result_items_visible: 0,
                        latest_tool_result_turn_index: None,
                        latest_tool_names_visible: Vec::new(),
                        active_code_window_trace: Vec::new(),
                        delivery_target: Some("runtime_execution_input".to_string()),
                    },
                    correlation.clone(),
                ),
                "2026-03-10T16:00:00.090Z",
            ),
            with_timestamp(
                Event::new(
                    EventPayload::ToolCallRequested {
                        native_correlation: flow.clone(),
                        task_id: Some(flow.task_id),
                        invocation_id: "inv-startup".to_string(),
                        turn_index: 0,
                        call_id: "call-1".to_string(),
                        tool_name: "read_file".to_string(),
                        request: test_blob(),
                        policy_tags: Vec::new(),
                    },
                    correlation.clone(),
                ),
                "2026-03-10T16:00:00.120Z",
            ),
            with_timestamp(
                Event::new(
                    EventPayload::AgentInvocationCompleted {
                        native_correlation: flow,
                        invocation_id: "inv-startup".to_string(),
                        total_turns: 0,
                        final_state: "done".to_string(),
                        success: true,
                        final_summary: Some("done".to_string()),
                        error_code: None,
                        error_message: None,
                        recoverable: None,
                    },
                    correlation,
                ),
                "2026-03-10T16:00:00.140Z",
            ),
        ];

        let summary = build_native_summary(&events);
        let invocation = &summary.invocations[0];
        assert_eq!(invocation.runtime_to_invocation_ms, Some(50));
        assert_eq!(invocation.invocation_to_first_model_request_ms, Some(40));
        assert_eq!(invocation.invocation_to_first_tool_call_ms, Some(70));
        assert_eq!(invocation.runtime_output_chunk_count, 1);
        assert_eq!(invocation.startup_progress.len(), 1);
        assert_eq!(
            invocation.startup_progress[0].stage.as_deref(),
            Some("invocation_starting")
        );
        assert_eq!(invocation.startup_progress[0].elapsed_ms, Some(12));
    }

    #[test]
    fn native_summary_explains_suspicious_success_runs() {
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
                    invocation_id: "inv-suspicious".to_string(),
                    adapter_name: "native".to_string(),
                    provider: "test".to_string(),
                    model: "mock".to_string(),
                    runtime_version: "1".to_string(),
                    capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                    agent_mode: Some("task_executor".to_string()),
                    allowed_tools: vec!["run_command".to_string(), "write_file".to_string()],
                    allowed_capabilities: vec!["read".to_string(), "write".to_string()],
                    configured_max_turns: Some(8),
                    configured_timeout_budget_ms: Some(60_000),
                    configured_token_budget: Some(8_000),
                    configured_prompt_headroom: Some(1_024),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallRequested {
                    native_correlation: flow.clone(),
                    task_id: Some(flow.task_id),
                    invocation_id: "inv-suspicious".to_string(),
                    turn_index: 1,
                    call_id: "call-test".to_string(),
                    tool_name: "run_command".to_string(),
                    request: json_blob(serde_json::json!({
                        "tool": "run_command",
                        "input": { "command": "cargo test" }
                    })),
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallCompleted {
                    native_correlation: flow.clone(),
                    task_id: Some(flow.task_id),
                    invocation_id: "inv-suspicious".to_string(),
                    turn_index: 1,
                    call_id: "call-test".to_string(),
                    tool_name: "run_command".to_string(),
                    response: json_blob(serde_json::json!({
                        "ok": true,
                        "output": { "exit_code": 101 }
                    })),
                    duration_ms: Some(1),
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallRequested {
                    native_correlation: flow.clone(),
                    task_id: Some(flow.task_id),
                    invocation_id: "inv-suspicious".to_string(),
                    turn_index: 2,
                    call_id: "call-write-1".to_string(),
                    tool_name: "write_file".to_string(),
                    request: json_blob(serde_json::json!({
                        "tool": "write_file",
                        "input": {
                            "path": "src/native/prompt_assembly.rs",
                            "append": false,
                            "content": "pub struct PromptProvenance;"
                        }
                    })),
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallRequested {
                    native_correlation: flow.clone(),
                    task_id: Some(flow.task_id),
                    invocation_id: "inv-suspicious".to_string(),
                    turn_index: 3,
                    call_id: "call-write-2".to_string(),
                    tool_name: "write_file".to_string(),
                    request: json_blob(serde_json::json!({
                        "tool": "write_file",
                        "input": {
                            "path": "src/native/turn_items.rs",
                            "append": false,
                            "content": "// rewritten"
                        }
                    })),
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::ToolCallFailed {
                    native_correlation: flow.clone(),
                    task_id: Some(flow.task_id),
                    invocation_id: "inv-suspicious".to_string(),
                    turn_index: 4,
                    call_id: "call-hard-fail".to_string(),
                    tool_name: "run_command".to_string(),
                    code: "native_tool_execution_failed".to_string(),
                    message: "failed to execute cargo test".to_string(),
                    recoverable: false,
                    duration_ms: Some(1),
                    policy_source: None,
                    denial_reason: None,
                    recovery_hint: None,
                    policy_tags: Vec::new(),
                },
                correlation.clone(),
            ),
            Event::new(
                EventPayload::AgentInvocationCompleted {
                    native_correlation: flow,
                    invocation_id: "inv-suspicious".to_string(),
                    total_turns: 4,
                    final_state: "done".to_string(),
                    success: true,
                    final_summary: Some(
                        "Implemented changes and verified with cargo test".to_string(),
                    ),
                    error_code: None,
                    error_message: None,
                    recoverable: None,
                },
                correlation,
            ),
        ];

        let summary = build_native_summary(&events);
        let invocation = &summary.invocations[0];
        assert!(!summary.verification.passed);
        assert_eq!(invocation.validation_attempts.len(), 1);
        assert_eq!(invocation.validation_attempts[0].outcome, "nonzero_exit");
        assert_eq!(invocation.nonrecoverable_tool_failures.len(), 1);
        assert_eq!(invocation.whole_file_overwrites.len(), 2);
        assert!(invocation
            .suspicious_signals
            .iter()
            .any(|signal| signal.contains("validation success without any passing")));
    }
}
