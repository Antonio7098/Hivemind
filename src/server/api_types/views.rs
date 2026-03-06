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
    pub(crate) diff: Option<String>,
}
