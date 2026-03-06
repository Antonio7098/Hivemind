//! Type definitions for the registry module.
//!
//! This module contains all the DTOs, result structs, and enums
//! used throughout the registry functionality.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeExecuteMode {
    #[default]
    Local,
    Pr,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MergeExecuteOptions {
    #[serde(default)]
    pub mode: MergeExecuteMode,
    #[serde(default)]
    pub monitor_ci: bool,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default)]
    pub pull_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeListEntry {
    pub adapter_name: String,
    pub default_binary: String,
    pub available: bool,
    pub opencode_compatible: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealthStatus {
    pub adapter_name: String,
    pub binary_path: String,
    pub healthy: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub selection_source: Option<crate::core::events::RuntimeSelectionSource>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}

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
#[serde(deny_unknown_fields)]
pub struct ConstitutionCompatibility {
    pub minimum_hivemind_version: String,
    pub governance_schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionPartition {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstitutionSeverity {
    Hard,
    Advisory,
    Informational,
}

impl ConstitutionSeverity {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Hard => "hard",
            Self::Advisory => "advisory",
            Self::Informational => "informational",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstitutionRule {
    ForbiddenDependency {
        id: String,
        from: String,
        to: String,
        severity: ConstitutionSeverity,
    },
    AllowedDependency {
        id: String,
        from: String,
        to: String,
        severity: ConstitutionSeverity,
    },
    CoverageRequirement {
        id: String,
        target: String,
        threshold: u8,
        severity: ConstitutionSeverity,
    },
}

impl ConstitutionRule {
    pub(crate) fn id(&self) -> &str {
        match self {
            Self::ForbiddenDependency { id, .. }
            | Self::AllowedDependency { id, .. }
            | Self::CoverageRequirement { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionArtifact {
    pub version: u32,
    pub schema_version: String,
    pub compatibility: ConstitutionCompatibility,
    #[serde(default)]
    pub partitions: Vec<ConstitutionPartition>,
    #[serde(default)]
    pub rules: Vec<ConstitutionRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstitutionValidationIssue {
    pub code: String,
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionShowResult {
    pub project_id: Uuid,
    pub path: String,
    pub revision: u64,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    pub compatibility: ConstitutionCompatibility,
    pub partitions: Vec<ConstitutionPartition>,
    pub rules: Vec<ConstitutionRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionMutationResult {
    pub project_id: Uuid,
    pub path: String,
    pub revision: u64,
    pub digest: String,
    #[serde(default)]
    pub previous_digest: Option<String>,
    pub schema_version: String,
    pub constitution_version: u32,
    pub actor: String,
    pub mutation_intent: String,
    pub confirmed: bool,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionValidationResult {
    pub project_id: Uuid,
    pub path: String,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    pub valid: bool,
    pub issues: Vec<ConstitutionValidationIssue>,
    pub validated_by: String,
    pub validated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstitutionRuleViolation {
    pub rule_id: String,
    pub rule_type: String,
    pub severity: ConstitutionSeverity,
    pub message: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub remediation_hint: Option<String>,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectConstitutionCheckResult {
    pub project_id: Uuid,
    pub gate: String,
    pub path: String,
    pub digest: String,
    pub schema_version: String,
    pub constitution_version: u32,
    #[serde(default)]
    pub flow_id: Option<Uuid>,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub attempt_id: Option<Uuid>,
    #[serde(default)]
    pub skipped: bool,
    #[serde(default)]
    pub skip_reason: Option<String>,
    #[serde(default)]
    pub violations: Vec<ConstitutionRuleViolation>,
    pub hard_violations: usize,
    pub advisory_violations: usize,
    pub informational_violations: usize,
    pub blocked: bool,
    pub checked_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSkillSummary {
    pub skill_id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSkillInspectResult {
    pub summary: GlobalSkillSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSystemPromptSummary {
    pub prompt_id: String,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalSystemPromptInspectResult {
    pub summary: GlobalSystemPromptSummary,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalTemplateSummary {
    pub template_id: String,
    pub system_prompt_id: String,
    pub skill_ids: Vec<String>,
    pub document_ids: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalTemplateInspectResult {
    pub summary: GlobalTemplateSummary,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateInstantiationResult {
    pub project_id: Uuid,
    pub template_id: String,
    pub system_prompt_id: String,
    pub skill_ids: Vec<String>,
    pub document_ids: Vec<String>,
    pub schema_version: String,
    pub projection_version: u32,
    pub instantiated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDocumentRevision {
    pub revision: u64,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

// Internal artifact types (not part of public API)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalSkillArtifact {
    pub skill_id: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalSystemPromptArtifact {
    pub prompt_id: String,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GlobalTemplateArtifact {
    pub template_id: String,
    pub system_prompt_id: String,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    #[serde(default)]
    pub document_ids: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub updated_at: chrono::DateTime<Utc>,
}
