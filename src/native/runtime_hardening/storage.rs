use super::*;

mod ingestor;
mod store;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(crate) struct NativeRuntimeStateStore {
    pub(crate) db_path: PathBuf,
    busy_timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphCodeArtifactRecord {
    pub registry_key: String,
    pub project_id: String,
    pub substrate_kind: String,
    pub storage_backend: String,
    pub storage_reference: String,
    pub derivative_snapshot_path: Option<String>,
    pub constitution_path: Option<String>,
    pub canonical_fingerprint: String,
    pub profile_version: String,
    pub ucp_engine_version: String,
    pub extractor_version: String,
    pub runtime_version: String,
    pub freshness_state: String,
    pub repo_manifest_json: String,
    pub active_session_ref: Option<String>,
    pub snapshot_json: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct GraphCodeArtifactUpsert {
    pub registry_key: String,
    pub project_id: String,
    pub substrate_kind: String,
    pub storage_backend: String,
    pub storage_reference: String,
    pub derivative_snapshot_path: Option<String>,
    pub constitution_path: Option<String>,
    pub canonical_fingerprint: String,
    pub profile_version: String,
    pub ucp_engine_version: String,
    pub extractor_version: String,
    pub runtime_version: String,
    pub freshness_state: String,
    pub repo_manifest_json: String,
    pub active_session_ref: Option<String>,
    pub snapshot_json: String,
}

#[derive(Debug, Clone)]
pub(crate) struct GraphCodeSessionUpsert {
    pub session_ref: String,
    pub registry_key: String,
    pub substrate_kind: String,
    pub current_focus_json: String,
    pub pinned_nodes_json: String,
    pub recent_traversals_json: String,
    pub working_set_refs_json: String,
    pub hydrated_excerpts_json: String,
    pub path_artifacts_json: String,
    pub snapshot_fingerprint: String,
    pub freshness_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GraphCodeSessionRecord {
    pub session_ref: String,
    pub registry_key: String,
    pub substrate_kind: String,
    pub current_focus_json: String,
    pub pinned_nodes_json: String,
    pub recent_traversals_json: String,
    pub working_set_refs_json: String,
    pub hydrated_excerpts_json: String,
    pub path_artifacts_json: String,
    pub snapshot_fingerprint: String,
    pub freshness_state: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeLogRecord {
    pub(crate) ts_ms: i64,
    pub(crate) component: String,
    pub(crate) level: String,
    pub(crate) message: String,
    pub(crate) context_json: Option<String>,
}

enum LogIngestorMessage {
    Log(RuntimeLogRecord),
    Flush(Sender<Result<()>>),
    Shutdown(Sender<Result<()>>),
}

#[derive(Debug)]
pub(super) struct RuntimeLogIngestor {
    sender: Sender<LogIngestorMessage>,
    handle: Option<thread::JoinHandle<()>>,
}
