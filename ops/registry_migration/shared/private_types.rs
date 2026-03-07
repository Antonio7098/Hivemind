// AUTO-GENERATED private helper types

// DiffArtifact (158-163)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiffArtifact {
    diff: Diff,
    unified: String,
}

// ScopeRepoSnapshot (164-172)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScopeRepoSnapshot {
    repo_path: String,
    #[serde(default)]
    git_head: Option<String>,
    #[serde(default)]
    status_lines: Vec<String>,
}

// ScopeBaselineArtifact (173-181)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScopeBaselineArtifact {
    attempt_id: Uuid,
    #[serde(default)]
    repo_snapshots: Vec<ScopeRepoSnapshot>,
    #[serde(default)]
    tmp_entries: Vec<String>,
}

// CheckpointCommitSpec (188-197)

struct CheckpointCommitSpec<'a> {
    flow_id: Uuid,
    task_id: Uuid,
    attempt_id: Uuid,
    checkpoint_id: &'a str,
    order: u32,
    total: u32,
    summary: Option<&'a str>,
}

// GraphSnapshotRepositoryCommit (237-243)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphSnapshotRepositoryCommit {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
}

// GraphSnapshotProvenance (244-250)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphSnapshotProvenance {
    project_id: Uuid,
    head_commits: Vec<GraphSnapshotRepositoryCommit>,
    generated_at: chrono::DateTime<Utc>,
}

// GraphSnapshotRepositoryArtifact (265-276)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphSnapshotRepositoryArtifact {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
    profile_version: String,
    canonical_fingerprint: String,
    stats: ucp_api::CodeGraphStats,
    document: PortableDocument,
    structure_blocks_projection: String,
}

// GraphSnapshotArtifact (277-289)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GraphSnapshotArtifact {
    schema_version: String,
    snapshot_version: u32,
    provenance: GraphSnapshotProvenance,
    ucp_engine_version: String,
    profile_version: String,
    canonical_fingerprint: String,
    summary: GraphSnapshotSummary,
    repositories: Vec<GraphSnapshotRepositoryArtifact>,
    static_projection: String,
}

// RuntimeGraphQueryGateError (290-297)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphQueryGateError {
    code: String,
    message: String,
    #[serde(default)]
    hint: Option<String>,
}

// GraphFileFact (797-804)

#[derive(Debug, Clone)]
struct GraphFileFact {
    display_path: String,
    path: String,
    symbol_key: String,
    references: Vec<String>,
}

// GraphConstitutionFacts (805-810)

#[derive(Debug, Clone, Default)]
struct GraphConstitutionFacts {
    files: HashMap<String, GraphFileFact>,
    symbol_file_keys: HashSet<String>,
}

// ProjectDocumentArtifact (874-885)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectDocumentArtifact {
    pub document_id: String,
    pub title: String,
    pub owner: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: chrono::DateTime<Utc>,
    #[serde(default)]
    pub revisions: Vec<ProjectDocumentRevision>,
}

// GlobalSkillArtifact (886-895)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalSkillArtifact {
    pub skill_id: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

// GlobalSystemPromptArtifact (896-902)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalSystemPromptArtifact {
    pub prompt_id: String,
    pub content: String,
    pub updated_at: chrono::DateTime<Utc>,
}

// GlobalTemplateArtifact (903-915)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalTemplateArtifact {
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

// TemplateInstantiationSnapshot (916-926)

#[derive(Debug, Clone)]
struct TemplateInstantiationSnapshot {
    event_id: String,
    template_id: String,
    system_prompt_id: String,
    skill_ids: Vec<String>,
    document_ids: Vec<String>,
    schema_version: String,
    projection_version: u32,
}

// GovernanceRecoverySnapshotEntry (927-940)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceRecoverySnapshotEntry {
    path: String,
    scope: String,
    artifact_kind: String,
    artifact_key: String,
    #[serde(default)]
    project_id: Option<Uuid>,
    #[serde(default)]
    revision: u64,
    content_hash: String,
    content: String,
}

// GovernanceRecoverySnapshotManifest (941-953)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GovernanceRecoverySnapshotManifest {
    schema_version: String,
    snapshot_id: String,
    project_id: Uuid,
    created_at: chrono::DateTime<Utc>,
    #[serde(default)]
    source_event_sequence: Option<u64>,
    artifact_count: usize,
    total_bytes: u64,
    artifacts: Vec<GovernanceRecoverySnapshotEntry>,
}

// GovernanceRepairInternalOp (954-967)

#[derive(Debug, Clone)]
enum GovernanceRepairInternalOp {
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
struct GovernanceRepairPlanBundle {
    result: ProjectGovernanceRepairPlanResult,
    operations: Vec<GovernanceRepairInternalOp>,
}

// AttemptContextDocumentManifest (974-981)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextDocumentManifest {
    document_id: String,
    source: String,
    revision: u64,
    content_hash: String,
}

// AttemptContextSkillManifest (982-987)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextSkillManifest {
    skill_id: String,
    content_hash: String,
}

// AttemptContextSystemPromptManifest (988-993)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextSystemPromptManifest {
    prompt_id: String,
    content_hash: String,
}

// AttemptContextConstitutionManifest (994-1004)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextConstitutionManifest {
    path: String,
    #[serde(default)]
    revision: Option<u64>,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    content_hash: Option<String>,
}

// AttemptContextGraphManifest (1005-1016)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextGraphManifest {
    present: bool,
    #[serde(default)]
    canonical_fingerprint: Option<String>,
    repository_count: usize,
    total_nodes: usize,
    total_edges: usize,
    #[serde(default)]
    languages: BTreeMap<String, usize>,
}

// AttemptContextRetryLink (1017-1023)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextRetryLink {
    attempt_id: Uuid,
    #[serde(default)]
    manifest_hash: Option<String>,
}

// AttemptContextBudgetManifest (1024-1040)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextBudgetManifest {
    total_budget_bytes: usize,
    default_section_budget_bytes: usize,
    #[serde(default)]
    per_section_budget_bytes: BTreeMap<String, usize>,
    max_expand_depth: usize,
    deduplicate: bool,
    original_size_bytes: usize,
    context_size_bytes: usize,
    #[serde(default)]
    truncated_sections: Vec<String>,
    #[serde(default)]
    truncation_reasons: BTreeMap<String, Vec<String>>,
    policy: String,
}

// AttemptContextManifest (1041-1067)

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AttemptContextManifest {
    schema_version: String,
    manifest_version: u32,
    ordered_inputs: Vec<String>,
    excluded_sources: Vec<String>,
    constitution: AttemptContextConstitutionManifest,
    #[serde(default)]
    template_id: Option<String>,
    #[serde(default)]
    template_event_id: Option<String>,
    #[serde(default)]
    template_schema_version: Option<String>,
    #[serde(default)]
    template_projection_version: Option<u32>,
    #[serde(default)]
    system_prompt: Option<AttemptContextSystemPromptManifest>,
    #[serde(default)]
    skills: Vec<AttemptContextSkillManifest>,
    #[serde(default)]
    documents: Vec<AttemptContextDocumentManifest>,
    graph_summary: AttemptContextGraphManifest,
    #[serde(default)]
    retry_links: Vec<AttemptContextRetryLink>,
    budget: AttemptContextBudgetManifest,
}

// AttemptContextBuildResult (1068-1088)

struct AttemptContextBuildResult {
    manifest_json: String,
    manifest_hash: String,
    inputs_hash: String,
    context: String,
    context_hash: String,
    original_size_bytes: usize,
    context_size_bytes: usize,
    truncated_sections: Vec<String>,
    truncation_reasons: BTreeMap<String, Vec<String>>,
    prior_manifest_hashes: Vec<String>,
    context_window_id: String,
    context_window_state_hash: String,
    context_window_snapshot_json: String,
    context_window_ops: Vec<ContextOpRecord>,
    template_document_ids: Vec<String>,
    included_document_ids: Vec<String>,
    excluded_document_ids: Vec<String>,
    resolved_document_ids: Vec<String>,
}

// GovernanceArtifactLocation (1089-1098)

#[derive(Debug, Clone)]
struct GovernanceArtifactLocation {
    project_id: Option<Uuid>,
    scope: &'static str,
    artifact_kind: &'static str,
    artifact_key: String,
    is_dir: bool,
    path: PathBuf,
}

// LegacyGovernanceArtifactMapping (1099-1104)

#[derive(Debug, Clone)]
struct LegacyGovernanceArtifactMapping {
    source: PathBuf,
    destination: GovernanceArtifactLocation,
}

// SelectedRuntimeAdapter (1105-1112)

enum SelectedRuntimeAdapter {
    OpenCode(crate::adapters::opencode::OpenCodeAdapter),
    Codex(CodexAdapter),
    ClaudeCode(ClaudeCodeAdapter),
    Kilo(KiloAdapter),
    Native(NativeRuntimeAdapter),
}

// ClassifiedRuntimeError (1169-1178)

#[derive(Debug, Clone)]
struct ClassifiedRuntimeError {
    code: String,
    category: String,
    message: String,
    recoverable: bool,
    retryable: bool,
    rate_limited: bool,
}
