use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextDocumentManifest {
    pub(crate) document_id: String,
    pub(crate) source: String,
    pub(crate) revision: u64,
    pub(crate) content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextSkillManifest {
    pub(crate) skill_id: String,
    pub(crate) content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextSystemPromptManifest {
    pub(crate) prompt_id: String,
    pub(crate) content_hash: String,
}
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AttemptContextRetryLink {
    pub(crate) attempt_id: Uuid,
    #[serde(default)]
    pub(crate) manifest_hash: Option<String>,
}
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
