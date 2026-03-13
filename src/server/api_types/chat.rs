use super::*;
use crate::adapters::runtime::NativeTransportTelemetry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChatMode {
    Freeflow,
    Plan,
}

impl ChatMode {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Freeflow => "freeflow",
            Self::Plan => "plan",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChatHistoryRole {
    User,
    Assistant,
}

impl ChatHistoryRole {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatHistoryMessageInput {
    pub(crate) role: ChatHistoryRole,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatInvokeRequest {
    pub(crate) mode: ChatMode,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) task: Option<String>,
    #[serde(default)]
    pub(crate) flow: Option<String>,
    #[serde(default)]
    pub(crate) history: Vec<ChatHistoryMessageInput>,
    #[serde(default)]
    pub(crate) context: Option<String>,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) max_turns: Option<u32>,
    #[serde(default)]
    pub(crate) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(crate) token_budget: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatInvokeTurnView {
    pub(crate) turn_index: u32,
    pub(crate) from_state: String,
    pub(crate) to_state: String,
    pub(crate) directive_kind: String,
    pub(crate) directive_text: String,
    pub(crate) raw_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatInvokeResponse {
    pub(crate) request_id: String,
    pub(crate) mode: String,
    pub(crate) project_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) flow_id: Option<String>,
    pub(crate) runtime_selection_source: Option<String>,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) assistant_message: String,
    pub(crate) final_state: String,
    pub(crate) turns: Vec<ChatInvokeTurnView>,
    pub(crate) transport: NativeTransportTelemetry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatSessionCreateRequest {
    pub(crate) mode: ChatMode,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) project: Option<String>,
    #[serde(default)]
    pub(crate) task: Option<String>,
    #[serde(default)]
    pub(crate) flow: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatSessionMessageView {
    pub(crate) message_id: String,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) created_at: String,
    pub(crate) request_id: Option<String>,
    pub(crate) provider: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) final_state: Option<String>,
    pub(crate) runtime_selection_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatSessionSummaryView {
    pub(crate) session_id: String,
    pub(crate) mode: String,
    pub(crate) title: String,
    pub(crate) project_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) flow_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) message_count: usize,
    pub(crate) last_message_preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatSessionInspectView {
    pub(crate) session_id: String,
    pub(crate) mode: String,
    pub(crate) title: String,
    pub(crate) project_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) flow_id: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) messages: Vec<ChatSessionMessageView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatSessionSendRequest {
    pub(crate) session_id: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) context: Option<String>,
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) max_turns: Option<u32>,
    #[serde(default)]
    pub(crate) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(crate) token_budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ChatSessionSendResponse {
    pub(crate) session_id: String,
    pub(crate) user_message_id: String,
    pub(crate) assistant_message_id: String,
    pub(crate) response: ChatInvokeResponse,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatStreamChunkView {
    pub(crate) session_id: String,
    pub(crate) message_id: String,
    pub(crate) request_id: String,
    pub(crate) turn_index: u32,
    pub(crate) from_state: String,
    pub(crate) to_state: String,
    pub(crate) directive_kind: String,
    pub(crate) content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ChatStreamEvent {
    MessageAppended { message: ChatSessionMessageView },
    StreamChunk { chunk: ChatStreamChunkView },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChatStreamEnvelope {
    pub(crate) cursor: u64,
    pub(crate) session_id: String,
    pub(crate) event: ChatStreamEvent,
}
