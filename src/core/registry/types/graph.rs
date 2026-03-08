use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct GraphValidationResult {
    pub graph_id: Uuid,
    pub valid: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointCompletionResult {
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_id: Uuid,
    pub checkpoint_id: String,
    pub order: u32,
    pub total: u32,
    #[serde(default)]
    pub next_checkpoint_id: Option<String>,
    pub all_completed: bool,
    pub commit_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttemptListItem {
    pub attempt_id: Uuid,
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_number: u32,
    pub started_at: chrono::DateTime<Utc>,
    pub all_checkpoints_completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeCleanupResult {
    pub flow_id: Uuid,
    pub cleaned_worktrees: usize,
    pub forced: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeTurnRestoreResult {
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_id: Uuid,
    pub ordinal: u32,
    pub git_ref: Option<String>,
    pub commit_sha: Option<String>,
    pub worktree_path: PathBuf,
    pub branch: Option<String>,
    pub head_before: String,
    pub head_after: String,
    pub had_local_changes: bool,
    pub forced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphSnapshotSummary {
    pub total_nodes: usize,
    pub repository_nodes: usize,
    pub directory_nodes: usize,
    pub file_nodes: usize,
    pub symbol_nodes: usize,
    pub total_edges: usize,
    pub reference_edges: usize,
    pub export_edges: usize,
    #[serde(default)]
    pub languages: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphSnapshotRefreshResult {
    pub project_id: Uuid,
    pub path: String,
    pub trigger: String,
    pub revision: u64,
    pub repository_count: usize,
    pub ucp_engine_version: String,
    pub profile_version: String,
    pub canonical_fingerprint: String,
    pub summary: GraphSnapshotSummary,
    pub diff_detected: bool,
}
