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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTurnTrace {
    pub turn_index: u32,
    pub from_state: String,
    pub to_state: String,
    pub model_request: String,
    pub model_response: String,
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
    pub response: Option<String>,
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
