use serde::Serialize;

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
