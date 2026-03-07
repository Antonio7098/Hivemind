use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactInspect {
    pub scope: String,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub path: String,
    pub exists: bool,
    pub projected: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceMigrationSummary {
    pub from_layout: String,
    pub to_layout: String,
    pub migrated_paths: Vec<String>,
    pub rollback_hint: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub migrated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceInitResult {
    pub project_id: Uuid,
    pub root_path: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub created_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceMigrateResult {
    pub project_id: Uuid,
    pub from_layout: String,
    pub to_layout: String,
    pub migrated_paths: Vec<String>,
    pub rollback_hint: String,
    pub schema_version: String,
    pub projection_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceInspectResult {
    pub project_id: Uuid,
    pub root_path: String,
    pub initialized: bool,
    pub schema_version: String,
    pub projection_version: u32,
    pub export_import_boundary: String,
    pub worktree_base_dir: String,
    pub artifacts: Vec<GovernanceArtifactInspect>,
    pub migrations: Vec<GovernanceMigrationSummary>,
    pub legacy_candidates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceDiagnosticIssue {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub artifact_kind: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceDiagnosticsResult {
    pub project_id: Uuid,
    pub checked_at: chrono::DateTime<Utc>,
    pub healthy: bool,
    pub issue_count: usize,
    pub issues: Vec<GovernanceDiagnosticIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceProjectionEntry {
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub scope: String,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub path: String,
    pub revision: u64,
    pub exists_on_disk: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceReplayResult {
    pub project_id: Uuid,
    pub replayed_at: chrono::DateTime<Utc>,
    pub projection_count: usize,
    pub idempotent: bool,
    pub current_matches_replay: bool,
    pub projections: Vec<GovernanceProjectionEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceSnapshotSummary {
    pub snapshot_id: String,
    pub path: String,
    pub created_at: chrono::DateTime<Utc>,
    pub artifact_count: usize,
    pub total_bytes: u64,
    #[serde(default)]
    pub source_event_sequence: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceSnapshotCreateResult {
    pub project_id: Uuid,
    pub reused_existing: bool,
    #[serde(default)]
    pub interval_minutes: Option<u64>,
    pub snapshot: GovernanceSnapshotSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceSnapshotListResult {
    pub project_id: Uuid,
    pub snapshot_count: usize,
    pub snapshots: Vec<GovernanceSnapshotSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceSnapshotRestoreResult {
    pub project_id: Uuid,
    pub snapshot_id: String,
    pub path: String,
    pub artifact_count: usize,
    pub restored_files: usize,
    pub skipped_files: usize,
    pub stale_files: usize,
    pub repaired_projection_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceDriftIssue {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub artifact_kind: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceRepairOperation {
    pub action: String,
    pub path: String,
    #[serde(default)]
    pub artifact_kind: Option<String>,
    #[serde(default)]
    pub artifact_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceRepairPlanResult {
    pub project_id: Uuid,
    pub checked_at: chrono::DateTime<Utc>,
    pub healthy: bool,
    pub issue_count: usize,
    pub recoverable_issue_count: usize,
    pub unrecoverable_issue_count: usize,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    pub ready_to_apply: bool,
    pub issues: Vec<GovernanceDriftIssue>,
    pub operations: Vec<GovernanceRepairOperation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceRepairApplyResult {
    pub project_id: Uuid,
    pub applied_at: chrono::DateTime<Utc>,
    #[serde(default)]
    pub snapshot_id: Option<String>,
    pub operation_count: usize,
    pub applied_operations: Vec<GovernanceRepairOperation>,
    pub remaining_issue_count: usize,
    pub remaining_issues: Vec<GovernanceDriftIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventLogIntegritySummary {
    pub event_count: usize,
    #[serde(default)]
    pub sequence_min: Option<u64>,
    #[serde(default)]
    pub sequence_max: Option<u64>,
    pub sequence_gap_count: usize,
    pub duplicate_event_id_count: usize,
    pub missing_sequence_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventsVerifyResult {
    pub checked_at: chrono::DateTime<Utc>,
    pub sqlite_path: String,
    pub mirror_path: String,
    pub parity_ok: bool,
    #[serde(default)]
    pub first_mismatch_index: Option<usize>,
    #[serde(default)]
    pub first_mismatch_sqlite_event_id: Option<String>,
    #[serde(default)]
    pub first_mismatch_mirror_event_id: Option<String>,
    pub sqlite: EventLogIntegritySummary,
    pub mirror: EventLogIntegritySummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventsRecoverResult {
    pub recovered_at: chrono::DateTime<Utc>,
    pub source: String,
    pub sqlite_path: String,
    pub mirror_path: String,
    #[serde(default)]
    pub backup_path: Option<String>,
    pub recovered_event_count: usize,
    pub verification: EventsVerifyResult,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceDocumentSummary {
    pub project_id: Uuid,
    pub document_id: String,
    pub title: String,
    pub owner: String,
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    pub revision: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceDocumentInspectResult {
    pub summary: ProjectGovernanceDocumentSummary,
    pub revisions: Vec<ProjectDocumentRevision>,
    pub latest_content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceDocumentWriteResult {
    pub project_id: Uuid,
    pub document_id: String,
    pub revision: u64,
    pub path: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactDeleteResult {
    pub project_id: Option<Uuid>,
    pub scope: String,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub path: String,
    pub schema_version: String,
    pub projection_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceAttachmentUpdateResult {
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub attached: bool,
    pub schema_version: String,
    pub projection_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceNotepadResult {
    pub scope: String,
    pub project_id: Option<Uuid>,
    pub path: String,
    pub exists: bool,
    pub content: Option<String>,
    pub non_executional: bool,
    pub non_validating: bool,
    pub schema_version: String,
    pub projection_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDocumentRevision {
    pub revision: u64,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ProjectDocumentArtifact {
    pub document_id: String,
    pub title: String,
    pub owner: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    #[serde(default)]
    pub revisions: Vec<ProjectDocumentRevision>,
}
