use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotRepositoryCommit {
    pub(crate) repo_name: String,
    pub(crate) repo_path: String,
    pub(crate) commit_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GraphSnapshotProvenance {
    pub(crate) project_id: Uuid,
    pub(crate) head_commits: Vec<GraphSnapshotRepositoryCommit>,
    pub(crate) generated_at: chrono::DateTime<Utc>,
}
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphQueryGateError {
    pub(crate) code: String,
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) hint: Option<String>,
}
#[derive(Debug, Clone)]
pub(crate) struct GraphFileFact {
    pub(crate) display_path: String,
    pub(crate) path: String,
    pub(crate) symbol_key: String,
    pub(crate) references: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub(crate) struct GraphConstitutionFacts {
    pub(crate) files: HashMap<String, GraphFileFact>,
    pub(crate) symbol_file_keys: HashSet<String>,
}
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
#[derive(Debug, Clone)]
pub(crate) struct GovernanceRepairPlanBundle {
    pub(crate) result: ProjectGovernanceRepairPlanResult,
    pub(crate) operations: Vec<GovernanceRepairInternalOp>,
}
#[derive(Debug, Clone)]
pub(crate) struct GovernanceArtifactLocation {
    pub(crate) project_id: Option<Uuid>,
    pub(crate) scope: &'static str,
    pub(crate) artifact_kind: &'static str,
    pub(crate) artifact_key: String,
    pub(crate) is_dir: bool,
    pub(crate) path: PathBuf,
}
#[derive(Debug, Clone)]
pub(crate) struct LegacyGovernanceArtifactMapping {
    pub(crate) source: PathBuf,
    pub(crate) destination: GovernanceArtifactLocation,
}
