use super::*;
use chrono::DateTime;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamDetailLevel {
    Summary,
    Observability,
    #[default]
    Telemetry,
}

impl RuntimeStreamDetailLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Observability => "observability",
            Self::Telemetry => "telemetry",
        }
    }

    pub fn includes_kind(self, kind: &str) -> bool {
        match self {
            Self::Summary => matches!(
                kind,
                "session"
                    | "turn"
                    | "interrupt"
                    | "runtime_exited"
                    | "runtime_terminated"
                    | "runtime_error"
                    | "recovery"
                    | "approval"
                    | "tool_call_failed"
                    | "checkpoint_declared"
                    | "checkpoint_activated"
                    | "checkpoint_completed"
                    | "checkpoint_all_completed"
                    | "checkpoint_commit"
            ),
            Self::Observability => matches!(
                kind,
                "session"
                    | "turn"
                    | "interrupt"
                    | "runtime_exited"
                    | "runtime_terminated"
                    | "runtime_error"
                    | "recovery"
                    | "filesystem"
                    | "command"
                    | "tool_call"
                    | "approval"
                    | "tool_call_requested"
                    | "tool_call_started"
                    | "tool_call_completed"
                    | "tool_call_failed"
                    | "todo"
                    | "checkpoint_declared"
                    | "checkpoint_activated"
                    | "checkpoint_completed"
                    | "checkpoint_all_completed"
                    | "checkpoint_commit"
            ),
            Self::Telemetry => true,
        }
    }
}

impl std::str::FromStr for RuntimeStreamDetailLevel {
    type Err = &'static str;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw {
            "summary" => Ok(Self::Summary),
            "observability" => Ok(Self::Observability),
            "telemetry" => Ok(Self::Telemetry),
            _ => Err("expected one of: summary, observability, telemetry"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptRuntimeSessionView {
    pub adapter_name: String,
    pub session_id: String,
    pub discovered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTurnRefView {
    pub ordinal: u32,
    pub adapter_name: String,
    pub stream: String,
    pub provider_session_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub git_ref: Option<String>,
    pub commit_sha: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AttemptRuntimeProjection {
    pub runtime_session: Option<AttemptRuntimeSessionView>,
    #[serde(default)]
    pub turn_refs: Vec<AttemptTurnRefView>,
    #[serde(default)]
    pub approvals: Vec<RuntimeApprovalView>,
    #[serde(default)]
    pub pending_approvals: Vec<RuntimeApprovalView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeApprovalView {
    pub approval_id: String,
    pub call_id: String,
    pub invocation_id: String,
    pub turn_index: u32,
    pub tool_name: String,
    pub approval_kind: String,
    pub status: String,
    pub resource: Option<String>,
    pub decision: Option<String>,
    pub summary: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub policy_tags: Vec<String>,
}

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStreamItemView {
    pub event_id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub flow_id: Option<String>,
    pub task_id: Option<String>,
    pub attempt_id: Option<String>,
    pub kind: String,
    pub stream: Option<String>,
    pub title: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub data: serde_json::Map<String, Value>,
}
