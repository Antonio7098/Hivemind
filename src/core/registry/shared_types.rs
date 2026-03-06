//! Auto-generated shared helper types for registry split modules.
#![allow(clippy::doc_markdown, clippy::wildcard_imports, unused_imports)]

use crate::core::registry::shared_prelude::*;
use crate::core::registry::types::*;
use crate::core::registry::{Registry, RegistryConfig};

// DiffArtifact (158-163)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiffArtifact {
    pub(crate) diff: Diff,
    pub(crate) unified: String,
}

// ScopeRepoSnapshot (164-172)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScopeRepoSnapshot {
    pub(crate) repo_path: String,
    #[serde(default)]
    pub(crate) git_head: Option<String>,
    #[serde(default)]
    pub(crate) status_lines: Vec<String>,
}

// ScopeBaselineArtifact (173-181)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ScopeBaselineArtifact {
    pub(crate) attempt_id: Uuid,
    #[serde(default)]
    pub(crate) repo_snapshots: Vec<ScopeRepoSnapshot>,
    #[serde(default)]
    pub(crate) tmp_entries: Vec<String>,
}

// CompletionArtifacts (182-187)

pub(crate) struct CompletionArtifacts<'a> {
    pub(crate) baseline_id: Uuid,
    pub(crate) artifact: &'a DiffArtifact,
    pub(crate) checkpoint_commit_sha: Option<String>,
}

// CheckpointCommitSpec (188-197)

pub(crate) struct CheckpointCommitSpec<'a> {
    pub(crate) flow_id: Uuid,
    pub(crate) task_id: Uuid,
    pub(crate) attempt_id: Uuid,
    pub(crate) checkpoint_id: &'a str,
    pub(crate) order: u32,
    pub(crate) total: u32,
    pub(crate) summary: Option<&'a str>,
}

// GraphSnapshotRepositoryCommit (237-243)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotRepositoryCommit {
    pub(crate) repo_name: String,
    pub(crate) repo_path: String,
    pub(crate) commit_hash: String,
}

// GraphSnapshotProvenance (244-250)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotProvenance {
    pub(crate) project_id: Uuid,
    pub(crate) head_commits: Vec<GraphSnapshotRepositoryCommit>,
    pub(crate) generated_at: chrono::DateTime<Utc>,
}

// GraphSnapshotRepositoryArtifact (265-276)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotRepositoryArtifact {
    pub(crate) repo_name: String,
    pub(crate) repo_path: String,
    pub(crate) commit_hash: String,
    pub(crate) profile_version: String,
    pub(crate) canonical_fingerprint: String,
    pub(crate) stats: ucp_api::CodeGraphStats,
    pub(crate) document: PortableDocument,
    pub(crate) structure_blocks_projection: String,
}

// GraphSnapshotArtifact (277-289)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotArtifact {
    pub(crate) schema_version: String,
    pub(crate) snapshot_version: u32,
    pub(crate) provenance: GraphSnapshotProvenance,
    pub(crate) ucp_engine_version: String,
    pub(crate) profile_version: String,
    pub(crate) canonical_fingerprint: String,
    pub(crate) summary: GraphSnapshotSummary,
    pub(crate) repositories: Vec<GraphSnapshotRepositoryArtifact>,
    pub(crate) static_projection: String,
}

// RuntimeGraphQueryGateError (290-297)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphQueryGateError {
    pub(crate) code: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) hint: Option<String>,
}

// GraphFileFact (797-804)

#[derive(Debug, Clone)]
pub(crate) struct GraphFileFact {
    pub(crate) display_path: String,
    pub(crate) path: String,
    pub(crate) symbol_key: String,
    pub(crate) references: Vec<String>,
}

// GraphConstitutionFacts (805-810)

#[derive(Debug, Clone, Default)]
pub(crate) struct GraphConstitutionFacts {
    pub(crate) files: HashMap<String, GraphFileFact>,
    pub(crate) symbol_file_keys: HashSet<String>,
}

// TemplateInstantiationSnapshot (916-926)

#[derive(Debug, Clone)]
pub(crate) struct TemplateInstantiationSnapshot {
    pub(crate) event_id: String,
    pub(crate) template_id: String,
    pub(crate) system_prompt_id: String,
    pub(crate) skill_ids: Vec<String>,
    pub(crate) document_ids: Vec<String>,
    pub(crate) schema_version: String,
    pub(crate) projection_version: u32,
}

// GovernanceRecoverySnapshotEntry (927-940)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GovernanceRecoverySnapshotEntry {
    pub(crate) path: String,
    pub(crate) scope: String,
    pub(crate) artifact_kind: String,
    pub(crate) artifact_key: String,
    #[serde(default)]
    pub(crate) project_id: Option<Uuid>,
    #[serde(default)]
    pub(crate) revision: u64,
    pub(crate) content_hash: String,
    pub(crate) content: String,
}

// GovernanceRecoverySnapshotManifest (941-953)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GovernanceRecoverySnapshotManifest {
    pub(crate) schema_version: String,
    pub(crate) snapshot_id: String,
    pub(crate) project_id: Uuid,
    pub(crate) created_at: chrono::DateTime<Utc>,
    #[serde(default)]
    pub(crate) source_event_sequence: Option<u64>,
    pub(crate) artifact_count: usize,
    pub(crate) total_bytes: u64,
    pub(crate) artifacts: Vec<GovernanceRecoverySnapshotEntry>,
}

// GovernanceRepairInternalOp (954-967)

#[derive(Debug, Clone)]
pub(crate) enum GovernanceRepairInternalOp {
    EmitUpsert {
        location: GovernanceArtifactLocation,
    },
    RestoreFromSnapshot {
        location: GovernanceArtifactLocation,
        entry: GovernanceRecoverySnapshotEntry,
    },
    RefreshGraphSnapshot {
        project_key: String,
    },
}

// GovernanceRepairPlanBundle (968-973)

#[derive(Debug, Clone)]
pub(crate) struct GovernanceRepairPlanBundle {
    pub(crate) result: ProjectGovernanceRepairPlanResult,
    pub(crate) operations: Vec<GovernanceRepairInternalOp>,
}

// AttemptContextDocumentManifest (974-981)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextDocumentManifest {
    pub(crate) document_id: String,
    pub(crate) source: String,
    pub(crate) revision: u64,
    pub(crate) content_hash: String,
}

// AttemptContextSkillManifest (982-987)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextSkillManifest {
    pub(crate) skill_id: String,
    pub(crate) content_hash: String,
}

// AttemptContextSystemPromptManifest (988-993)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextSystemPromptManifest {
    pub(crate) prompt_id: String,
    pub(crate) content_hash: String,
}

// AttemptContextConstitutionManifest (994-1004)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextConstitutionManifest {
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) revision: Option<u64>,
    #[serde(default)]
    pub(crate) digest: Option<String>,
    #[serde(default)]
    pub(crate) content_hash: Option<String>,
}

// AttemptContextGraphManifest (1005-1016)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextGraphManifest {
    pub(crate) present: bool,
    #[serde(default)]
    pub(crate) canonical_fingerprint: Option<String>,
    pub(crate) repository_count: usize,
    pub(crate) total_nodes: usize,
    pub(crate) total_edges: usize,
    #[serde(default)]
    pub(crate) languages: BTreeMap<String, usize>,
}

// AttemptContextRetryLink (1017-1023)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextRetryLink {
    pub(crate) attempt_id: Uuid,
    #[serde(default)]
    pub(crate) manifest_hash: Option<String>,
}

// AttemptContextBudgetManifest (1024-1040)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextBudgetManifest {
    pub(crate) total_budget_bytes: usize,
    pub(crate) default_section_budget_bytes: usize,
    #[serde(default)]
    pub(crate) per_section_budget_bytes: BTreeMap<String, usize>,
    pub(crate) max_expand_depth: usize,
    pub(crate) deduplicate: bool,
    pub(crate) original_size_bytes: usize,
    pub(crate) context_size_bytes: usize,
    #[serde(default)]
    pub(crate) truncated_sections: Vec<String>,
    #[serde(default)]
    pub(crate) truncation_reasons: BTreeMap<String, Vec<String>>,
    pub(crate) policy: String,
}

// AttemptContextManifest (1041-1067)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextManifest {
    pub(crate) schema_version: String,
    pub(crate) manifest_version: u32,
    pub(crate) ordered_inputs: Vec<String>,
    pub(crate) excluded_sources: Vec<String>,
    pub(crate) constitution: AttemptContextConstitutionManifest,
    #[serde(default)]
    pub(crate) template_id: Option<String>,
    #[serde(default)]
    pub(crate) template_event_id: Option<String>,
    #[serde(default)]
    pub(crate) template_schema_version: Option<String>,
    #[serde(default)]
    pub(crate) template_projection_version: Option<u32>,
    #[serde(default)]
    pub(crate) system_prompt: Option<AttemptContextSystemPromptManifest>,
    #[serde(default)]
    pub(crate) skills: Vec<AttemptContextSkillManifest>,
    #[serde(default)]
    pub(crate) documents: Vec<AttemptContextDocumentManifest>,
    pub(crate) graph_summary: AttemptContextGraphManifest,
    #[serde(default)]
    pub(crate) retry_links: Vec<AttemptContextRetryLink>,
    pub(crate) budget: AttemptContextBudgetManifest,
}

// AttemptContextBuildResult (1068-1088)

pub(crate) struct AttemptContextBuildResult {
    pub(crate) manifest_json: String,
    pub(crate) manifest_hash: String,
    pub(crate) inputs_hash: String,
    pub(crate) context: String,
    pub(crate) context_hash: String,
    pub(crate) original_size_bytes: usize,
    pub(crate) context_size_bytes: usize,
    pub(crate) truncated_sections: Vec<String>,
    pub(crate) truncation_reasons: BTreeMap<String, Vec<String>>,
    pub(crate) prior_manifest_hashes: Vec<String>,
    pub(crate) context_window_id: String,
    pub(crate) context_window_state_hash: String,
    pub(crate) context_window_snapshot_json: String,
    pub(crate) context_window_ops: Vec<ContextOpRecord>,
    pub(crate) template_document_ids: Vec<String>,
    pub(crate) included_document_ids: Vec<String>,
    pub(crate) excluded_document_ids: Vec<String>,
    pub(crate) resolved_document_ids: Vec<String>,
}

// GovernanceArtifactLocation (1089-1098)

#[derive(Debug, Clone)]
pub(crate) struct GovernanceArtifactLocation {
    pub(crate) project_id: Option<Uuid>,
    pub(crate) scope: &'static str,
    pub(crate) artifact_kind: &'static str,
    pub(crate) artifact_key: String,
    pub(crate) is_dir: bool,
    pub(crate) path: PathBuf,
}

// LegacyGovernanceArtifactMapping (1099-1104)

#[derive(Debug, Clone)]
pub(crate) struct LegacyGovernanceArtifactMapping {
    pub(crate) source: PathBuf,
    pub(crate) destination: GovernanceArtifactLocation,
}

// SelectedRuntimeAdapter (1105-1112)

pub(crate) enum SelectedRuntimeAdapter {
    OpenCode(crate::adapters::opencode::OpenCodeAdapter),
    Codex(CodexAdapter),
    ClaudeCode(ClaudeCodeAdapter),
    Kilo(KiloAdapter),
    Native(NativeRuntimeAdapter),
}

// ClassifiedRuntimeError (1169-1178)

#[derive(Debug, Clone)]
pub(crate) struct ClassifiedRuntimeError {
    pub(crate) code: String,
    pub(crate) category: String,
    pub(crate) message: String,
    pub(crate) recoverable: bool,
    pub(crate) retryable: bool,
    pub(crate) rate_limited: bool,
}

// SelectedRuntimeAdapter (1114-1168)
impl SelectedRuntimeAdapter {
    pub(crate) fn initialize(&mut self) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.initialize(),
            Self::Codex(a) => a.initialize(),
            Self::ClaudeCode(a) => a.initialize(),
            Self::Kilo(a) => a.initialize(),
            Self::Native(a) => a.initialize(),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        task_id: Uuid,
        worktree: &std::path::Path,
    ) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.prepare(task_id, worktree),
            Self::Codex(a) => a.prepare(task_id, worktree),
            Self::ClaudeCode(a) => a.prepare(task_id, worktree),
            Self::Kilo(a) => a.prepare(task_id, worktree),
            Self::Native(a) => a.prepare(task_id, worktree),
        }
    }

    pub(crate) fn execute(
        &mut self,
        input: ExecutionInput,
    ) -> std::result::Result<crate::adapters::runtime::ExecutionReport, RuntimeError> {
        match self {
            Self::OpenCode(a) => a.execute(input),
            Self::Codex(a) => a.execute(input),
            Self::ClaudeCode(a) => a.execute(input),
            Self::Kilo(a) => a.execute(input),
            Self::Native(a) => a.execute(input),
        }
    }

    pub(crate) fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        on_event: F,
    ) -> std::result::Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        match self {
            Self::OpenCode(a) => a.execute_interactive(input, on_event),
            Self::Codex(a) => a.execute_interactive(input, on_event),
            Self::ClaudeCode(a) => a.execute_interactive(input, on_event),
            Self::Kilo(a) => a.execute_interactive(input, on_event),
            Self::Native(a) => a.execute_interactive(input, on_event),
        }
    }
}
