use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiffArtifact {
    pub(crate) diff: Diff,
    pub(crate) unified: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScopeRepoSnapshot {
    pub(crate) repo_path: String,
    #[serde(default)]
    pub(crate) git_head: Option<String>,
    #[serde(default)]
    pub(crate) status_lines: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScopeBaselineArtifact {
    pub(crate) attempt_id: Uuid,
    #[serde(default)]
    pub(crate) repo_snapshots: Vec<ScopeRepoSnapshot>,
    #[serde(default)]
    pub(crate) tmp_entries: Vec<String>,
}
pub(crate) struct CompletionArtifacts<'a> {
    pub(crate) baseline_id: Uuid,
    pub(crate) artifact: &'a DiffArtifact,
    pub(crate) checkpoint_commit_sha: Option<String>,
}
pub(crate) struct CheckpointCommitSpec<'a> {
    pub(crate) flow_id: Uuid,
    pub(crate) task_id: Uuid,
    pub(crate) attempt_id: Uuid,
    pub(crate) checkpoint_id: &'a str,
    pub(crate) order: u32,
    pub(crate) total: u32,
    pub(crate) summary: Option<&'a str>,
}
