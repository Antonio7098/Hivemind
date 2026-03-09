use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NativePayloadCaptureMode {
    #[default]
    MetadataOnly,
    FullPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInvocationTrace {
    pub invocation_id: String,
    pub provider: String,
    pub model: String,
    pub runtime_version: String,
    #[serde(default)]
    pub capture_mode: NativePayloadCaptureMode,
    #[serde(default)]
    pub agent_mode: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub allowed_capabilities: Vec<String>,
    #[serde(default)]
    pub configured_max_turns: Option<u32>,
    #[serde(default)]
    pub configured_timeout_budget_ms: Option<u64>,
    #[serde(default)]
    pub configured_token_budget: Option<usize>,
    #[serde(default)]
    pub configured_prompt_headroom: Option<usize>,
    #[serde(default)]
    pub turns: Vec<NativeTurnTrace>,
    pub final_state: String,
    #[serde(default)]
    pub final_summary: Option<String>,
    #[serde(default)]
    pub failure: Option<NativeInvocationFailure>,
    #[serde(default)]
    pub transport: NativeTransportTelemetry,
    #[serde(default)]
    pub runtime_state: Option<NativeRuntimeStateTelemetry>,
    #[serde(default)]
    pub readiness_transitions: Vec<NativeReadinessTransition>,
    #[serde(default)]
    pub history_compactions: Vec<NativeHistoryCompactionTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTurnTrace {
    pub turn_index: u32,
    pub from_state: String,
    pub to_state: String,
    #[serde(default)]
    pub agent_mode: Option<String>,
    pub model_request: String,
    pub model_response: String,
    #[serde(default)]
    pub prompt_hash: Option<String>,
    #[serde(default)]
    pub context_manifest_hash: Option<String>,
    #[serde(default)]
    pub delivered_context_hash: Option<String>,
    #[serde(default)]
    pub prompt_headroom: Option<usize>,
    #[serde(default)]
    pub available_budget: usize,
    #[serde(default)]
    pub mode_contract_hash: Option<String>,
    #[serde(default)]
    pub inputs_hash: Option<String>,
    #[serde(default)]
    pub rendered_context_hash: Option<String>,
    #[serde(default)]
    pub context_window_state_hash: Option<String>,
    #[serde(default)]
    pub delivery_target: Option<String>,
    #[serde(default)]
    pub rendered_prompt_bytes: usize,
    #[serde(default)]
    pub runtime_context_bytes: usize,
    #[serde(default)]
    pub static_prompt_chars: usize,
    #[serde(default)]
    pub selected_history_chars: usize,
    #[serde(default)]
    pub active_code_window_chars: usize,
    #[serde(default)]
    pub compacted_summary_chars: usize,
    #[serde(default)]
    pub code_navigation_chars: usize,
    #[serde(default)]
    pub tool_contract_chars: usize,
    #[serde(default)]
    pub assembly_duration_ms: u64,
    #[serde(default)]
    pub visible_item_count: usize,
    #[serde(default)]
    pub selected_history_count: usize,
    #[serde(default)]
    pub active_code_window_count: usize,
    #[serde(default)]
    pub code_navigation_count: usize,
    #[serde(default)]
    pub compacted_summary_count: usize,
    #[serde(default)]
    pub tool_contract_count: usize,
    #[serde(default)]
    pub skipped_item_count: usize,
    #[serde(default)]
    pub truncated_item_count: usize,
    #[serde(default)]
    pub tool_result_items_visible: usize,
    #[serde(default)]
    pub latest_tool_result_turn_index: Option<u32>,
    #[serde(default)]
    pub latest_tool_names_visible: Vec<String>,
    #[serde(default)]
    pub tool_result_count: usize,
    #[serde(default)]
    pub budget_used_before: usize,
    #[serde(default)]
    pub budget_used_after: usize,
    #[serde(default)]
    pub budget_remaining: usize,
    #[serde(default)]
    pub budget_thresholds_crossed: Vec<u8>,
    #[serde(default)]
    pub model_latency_ms: u64,
    #[serde(default)]
    pub tool_latency_ms: u64,
    #[serde(default)]
    pub turn_duration_ms: u64,
    #[serde(default)]
    pub elapsed_since_invocation_ms: u64,
    #[serde(default)]
    pub request_tokens: usize,
    #[serde(default)]
    pub response_tokens: usize,
    #[serde(default)]
    pub tool_calls: Vec<NativeToolCallTrace>,
    #[serde(default)]
    pub turn_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolCallTrace {
    pub call_id: String,
    pub tool_name: String,
    pub request: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub response_original_bytes: Option<usize>,
    #[serde(default)]
    pub response_stored_bytes: Option<usize>,
    #[serde(default)]
    pub response_truncated: bool,
    #[serde(default)]
    pub failure: Option<NativeToolCallFailure>,
    #[serde(default)]
    pub policy_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolCallFailure {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub policy_source: Option<String>,
    #[serde(default)]
    pub denial_reason: Option<String>,
    #[serde(default)]
    pub recovery_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInvocationFailure {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub recovery_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeTransportTelemetry {
    #[serde(default)]
    pub attempts: Vec<NativeTransportAttemptTrace>,
    #[serde(default)]
    pub fallback_activations: Vec<NativeTransportFallbackTrace>,
    #[serde(default)]
    pub active_transport: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTransportAttemptTrace {
    pub turn_index: u32,
    pub attempt: u32,
    pub transport: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub rate_limited: bool,
    #[serde(default)]
    pub status_code: Option<u16>,
    #[serde(default)]
    pub backoff_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTransportFallbackTrace {
    pub turn_index: u32,
    pub from_transport: String,
    pub to_transport: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeRuntimeStateTelemetry {
    pub db_path: String,
    pub busy_timeout_ms: u64,
    pub log_batch_size: usize,
    pub log_retention_days: u64,
    pub lease_owner_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeReadinessTransition {
    pub component: String,
    pub token: String,
    pub from_state: String,
    pub to_state: String,
    pub reason: String,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeHistoryCompactionTrace {
    pub turn_index: u32,
    pub reason: String,
    #[serde(default)]
    pub rendered_prompt_bytes_before: usize,
    #[serde(default)]
    pub selected_history_count_before: usize,
    #[serde(default)]
    pub selected_history_chars_before: usize,
    #[serde(default)]
    pub visible_items_before: usize,
    #[serde(default)]
    pub visible_items_after: usize,
    #[serde(default)]
    pub prompt_tokens_before: usize,
    #[serde(default)]
    pub projected_budget_used: usize,
    #[serde(default)]
    pub token_budget: usize,
    #[serde(default)]
    pub elapsed_since_invocation_ms: u64,
}
