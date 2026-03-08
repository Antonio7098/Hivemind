use super::*;

#[derive(Debug, Serialize)]
pub(crate) struct VerifyResultsView {
    pub(crate) attempt_id: String,
    pub(crate) task_id: String,
    pub(crate) flow_id: String,
    pub(crate) attempt_number: u32,
    pub(crate) check_results: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub(crate) struct AttemptInspectView {
    pub(crate) attempt_id: String,
    pub(crate) task_id: String,
    pub(crate) flow_id: String,
    pub(crate) attempt_number: u32,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) baseline_id: Option<String>,
    pub(crate) diff_id: Option<String>,
    pub(crate) runtime_session: Option<AttemptRuntimeSessionView>,
    pub(crate) turn_refs: Vec<AttemptTurnRefView>,
    pub(crate) diff: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AttemptRuntimeSessionView {
    pub(crate) adapter_name: String,
    pub(crate) session_id: String,
    pub(crate) discovered_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AttemptTurnRefView {
    pub(crate) ordinal: u32,
    pub(crate) adapter_name: String,
    pub(crate) stream: String,
    pub(crate) provider_session_id: Option<String>,
    pub(crate) provider_turn_id: Option<String>,
    pub(crate) git_ref: Option<String>,
    pub(crate) commit_sha: Option<String>,
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeStreamItemView {
    pub(crate) event_id: String,
    pub(crate) sequence: u64,
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) flow_id: Option<String>,
    pub(crate) task_id: Option<String>,
    pub(crate) attempt_id: Option<String>,
    pub(crate) kind: String,
    pub(crate) stream: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) text: Option<String>,
    pub(crate) data: Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeStreamEnvelope {
    pub(crate) cursor: u64,
    pub(crate) item: RuntimeStreamItemView,
}
