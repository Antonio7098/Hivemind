use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct VerifyOverrideRequest {
    pub(crate) task_id: String,
    pub(crate) decision: String,
    pub(crate) reason: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VerifyRunRequest {
    pub(crate) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MergePrepareRequest {
    pub(crate) flow_id: String,
    pub(crate) target: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MergeApproveRequest {
    pub(crate) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MergeExecuteRequest {
    pub(crate) flow_id: String,
    pub(crate) mode: Option<String>,
    pub(crate) monitor_ci: Option<bool>,
    pub(crate) auto_merge: Option<bool>,
    pub(crate) pull_after: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CheckpointCompleteRequest {
    pub(crate) attempt_id: String,
    pub(crate) checkpoint_id: String,
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorktreeCleanupRequest {
    pub(crate) flow_id: String,
    #[serde(default)]
    pub(crate) force: bool,
    #[serde(default)]
    pub(crate) dry_run: bool,
}
