use super::env::resolve_repo_head_commit;
use super::errors::{graph_query_runtime_error, graph_query_runtime_error_with_refresh_hint};
use super::types::{
    PersistedGraphQueryNodeRef, PersistedGraphQueryTraversal, RuntimeGraphQueryGateError,
    RuntimeGraphSnapshotCommit, RuntimeGraphSnapshotProvenance,
};
use super::*;
use crate::core::graph_query::GraphQueryEdge;
use std::collections::BTreeSet;
use ucp_api::{
    build_code_graph_incremental, CodeGraphBuildInput, CodeGraphExtractorConfig,
    CodeGraphIncrementalBuildInput, CodeGraphIncrementalStats, PortableDocument,
};

const GRAPH_QUERY_EXECUTION_SCOPE: &str = "execution_worktree";
const GRAPH_QUERY_AUTHORITATIVE_SCOPE: &str = "authoritative_snapshot";
const HIVEMIND_ATTEMPT_ID_ENV: &str = "HIVEMIND_ATTEMPT_ID";
const HIVEMIND_PRIMARY_WORKTREE_ENV: &str = "HIVEMIND_PRIMARY_WORKTREE";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRuntimeGraphSnapshotMetadata {
    version: u32,
    profile_version: String,
    canonical_fingerprint: String,
    provenance: RuntimeGraphSnapshotProvenance,
    #[serde(default = "default_graph_query_scope")]
    graph_scope: String,
    #[serde(default)]
    dirty_paths: Vec<String>,
    #[serde(default)]
    refresh_reason: Option<String>,
    #[serde(default)]
    incremental_repos: Vec<StoredRuntimeGraphIncrementalRepository>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRuntimeGraphIncrementalRepository {
    repo_name: String,
    state_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stats: Option<CodeGraphIncrementalStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRuntimeGraphSnapshotRepositoryManifest {
    repo_name: String,
    repo_path: String,
    worktree_path: String,
    commit_hash: String,
    canonical_fingerprint: String,
    graph_key: String,
}

#[derive(Debug, Default)]
struct SessionImpactAnalysis {
    changed_node_ids: BTreeSet<String>,
    changed_logical_keys: BTreeSet<String>,
    changed_paths: BTreeSet<String>,
    current_node_ids: BTreeSet<String>,
}

fn default_graph_query_scope() -> String {
    GRAPH_QUERY_AUTHORITATIVE_SCOPE.to_string()
}

fn open_runtime_state_store(
    env: &HashMap<String, String>,
) -> Result<crate::native::runtime_hardening::NativeRuntimeStateStore, NativeToolEngineError> {
    crate::native::runtime_hardening::NativeRuntimeStateStore::open(
        &crate::native::runtime_hardening::RuntimeHardeningConfig::from_env(env),
    )
    .map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_registry_unavailable",
            format!(
                "failed to open GraphCode runtime registry: {}",
                error.message
            ),
        )
    })
}

fn runtime_graph_store_path(db_path: &Path, substrate_kind: &str) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("graph-store")
        .join(format!("{substrate_kind}.sqlite"))
}

fn runtime_graph_incremental_state_file(
    db_path: &Path,
    registry_key: &str,
    repo_name: &str,
) -> PathBuf {
    let sanitized_registry = registry_key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let sanitized_repo = repo_name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("graph-store")
        .join("incremental")
        .join(sanitized_registry)
        .join(format!("{sanitized_repo}.json"))
}

fn authoritative_registry_key(project_id: &str) -> String {
    format!("{project_id}:codegraph")
}

fn active_runtime_registry_key(project_id: &str, env: &HashMap<String, String>) -> String {
    env.get(HIVEMIND_ATTEMPT_ID_ENV)
        .filter(|value| !value.trim().is_empty())
        .map(|attempt_id| format!("{project_id}:codegraph:attempt:{attempt_id}"))
        .unwrap_or_else(|| authoritative_registry_key(project_id))
}

fn active_session_ref(registry_key: &str) -> String {
    format!("{registry_key}:session:default")
}

fn is_attempt_scoped_registry(project_id: &str, env: &HashMap<String, String>) -> bool {
    active_runtime_registry_key(project_id, env) != authoritative_registry_key(project_id)
}

fn sanitize_repo_env_suffix(repo_name: &str) -> String {
    repo_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn resolve_repo_worktree_path(
    repo_name: &str,
    default_repo_path: &str,
    env: &HashMap<String, String>,
) -> PathBuf {
    let specific_key = format!(
        "HIVEMIND_REPO_{}_WORKTREE",
        sanitize_repo_env_suffix(repo_name)
    );
    env.get(&specific_key)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env.get(HIVEMIND_PRIMARY_WORKTREE_ENV)
                .filter(|value| !value.trim().is_empty())
        })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_repo_path))
}

fn serialize_snapshot_metadata(
    metadata: &StoredRuntimeGraphSnapshotMetadata,
) -> Result<String, NativeToolEngineError> {
    serde_json::to_string(metadata).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_registry_invalid",
            format!("failed to encode graph snapshot metadata: {error}"),
        )
    })
}

fn parse_snapshot_metadata(raw: &str) -> Option<StoredRuntimeGraphSnapshotMetadata> {
    serde_json::from_str::<StoredRuntimeGraphSnapshotMetadata>(raw).ok()
}

fn parse_manifest(
    record: &crate::native::runtime_hardening::GraphCodeArtifactRecord,
    label: &str,
) -> Result<Vec<StoredRuntimeGraphSnapshotRepositoryManifest>, NativeToolEngineError> {
    serde_json::from_str::<Vec<StoredRuntimeGraphSnapshotRepositoryManifest>>(
        &record.repo_manifest_json,
    )
    .map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid external GraphCode manifest for {label}: {error}"),
        )
    })
}

fn persist_runtime_graph_documents(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    registry_key: &str,
    artifact: &RuntimeGraphSnapshotArtifact,
) -> Result<(String, String), NativeToolEngineError> {
    let storage_path = runtime_graph_store_path(&store.db_path, "codegraph");
    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_registry_invalid",
                format!(
                    "failed to create GraphCode runtime storage directory '{}': {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let manifest = artifact
        .repositories
        .iter()
        .enumerate()
        .map(|(index, repo)| {
            let graph_key = format!("{registry_key}:repo:{index}");
            ucp_graph::GraphNavigator::from_portable(&repo.document)
                .and_then(|navigator| navigator.persist_sqlite(&storage_path, &graph_key))
                .map_err(|error| {
                    graph_query_runtime_error_with_refresh_hint(
                        "graph_snapshot_registry_invalid",
                        format!(
                            "failed to persist GraphCode runtime document for repository '{}': {error}",
                            repo.repo_name
                        ),
                    )
                })?;
            Ok(StoredRuntimeGraphSnapshotRepositoryManifest {
                repo_name: repo.repo_name.clone(),
                repo_path: repo.repo_path.clone(),
                worktree_path: repo.repo_path.clone(),
                commit_hash: repo.commit_hash.clone(),
                canonical_fingerprint: repo.canonical_fingerprint.clone(),
                graph_key,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    serde_json::to_string(&manifest)
        .map(|raw| (storage_path.to_string_lossy().to_string(), raw))
        .map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_registry_invalid",
                format!("failed to encode GraphCode runtime manifest: {error}"),
            )
        })
}

fn load_runtime_graph_from_record(
    record: &crate::native::runtime_hardening::GraphCodeArtifactRecord,
    label: &str,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    let metadata = parse_snapshot_metadata(&record.snapshot_json).ok_or_else(|| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid external GraphCode metadata for {label}"),
        )
    })?;
    let repositories = parse_manifest(record, label)?
        .into_iter()
        .map(|repo| {
            let document =
                ucp_graph::GraphNavigator::open_sqlite(&record.storage_reference, &repo.graph_key)
                    .and_then(|navigator| navigator.to_portable_document())
                    .map_err(|error| {
                        graph_query_runtime_error_with_refresh_hint(
                            "graph_snapshot_invalid",
                            format!(
                        "failed to load GraphCode runtime document for repository '{}': {error}",
                        repo.repo_name
                    ),
                        )
                    })?;
            Ok(RuntimeGraphSnapshotRepository {
                repo_name: repo.repo_name,
                repo_path: repo.repo_path,
                commit_hash: repo.commit_hash,
                canonical_fingerprint: repo.canonical_fingerprint,
                document,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RuntimeGraphSnapshotArtifact {
        profile_version: metadata.profile_version,
        canonical_fingerprint: metadata.canonical_fingerprint,
        provenance: metadata.provenance,
        repositories,
    })
}

fn ensure_graphcode_session_exists(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    registry_key: &str,
    snapshot_fingerprint: &str,
) -> Result<(), NativeToolEngineError> {
    let session_ref = active_session_ref(registry_key);
    if store
        .graphcode_session_by_ref(&session_ref)
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?
        .is_some()
    {
        return Ok(());
    }
    store
        .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
            session_ref,
            registry_key: registry_key.to_string(),
            substrate_kind: "codegraph".to_string(),
            current_focus_json: "[]".to_string(),
            pinned_nodes_json: "[]".to_string(),
            recent_traversals_json: "[]".to_string(),
            working_set_refs_json: "[]".to_string(),
            hydrated_excerpts_json: "[]".to_string(),
            path_artifacts_json: "[]".to_string(),
            snapshot_fingerprint: snapshot_fingerprint.to_string(),
            freshness_state: "current".to_string(),
        })
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))
}

fn upsert_runtime_graph_artifact(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    project_id: &str,
    registry_key: &str,
    artifact: &RuntimeGraphSnapshotArtifact,
    derivative_snapshot_path: Option<String>,
    constitution_path: Option<String>,
    metadata: StoredRuntimeGraphSnapshotMetadata,
) -> Result<(), NativeToolEngineError> {
    let (storage_reference, repo_manifest_json) =
        persist_runtime_graph_documents(store, registry_key, artifact)?;
    let snapshot_json = serialize_snapshot_metadata(&metadata)?;
    store
        .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
            registry_key: registry_key.to_string(),
            project_id: project_id.to_string(),
            substrate_kind: "codegraph".to_string(),
            storage_backend: "ucp_graph_sqlite".to_string(),
            storage_reference,
            derivative_snapshot_path,
            constitution_path,
            canonical_fingerprint: artifact.canonical_fingerprint.clone(),
            profile_version: artifact.profile_version.clone(),
            ucp_engine_version: ucp_api::CODEGRAPH_EXTRACTOR_VERSION.to_string(),
            extractor_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            freshness_state: "current".to_string(),
            repo_manifest_json,
            active_session_ref: Some(active_session_ref(registry_key)),
            snapshot_json,
        })
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?;
    ensure_graphcode_session_exists(store, registry_key, &artifact.canonical_fingerprint)
}

fn sync_snapshot_into_runtime_registry(
    env: &HashMap<String, String>,
    snapshot_path: &str,
    artifact: &RuntimeGraphSnapshotArtifact,
) -> Result<(), NativeToolEngineError> {
    let Some(project_id) = env.get(GRAPH_QUERY_ENV_PROJECT_ID).cloned() else {
        return Ok(());
    };
    let store = open_runtime_state_store(env)?;
    let registry_key = authoritative_registry_key(&project_id);
    let metadata = StoredRuntimeGraphSnapshotMetadata {
        version: 2,
        profile_version: artifact.profile_version.clone(),
        canonical_fingerprint: artifact.canonical_fingerprint.clone(),
        provenance: artifact.provenance.clone(),
        graph_scope: GRAPH_QUERY_AUTHORITATIVE_SCOPE.to_string(),
        dirty_paths: Vec::new(),
        refresh_reason: Some("snapshot_sync".to_string()),
        incremental_repos: Vec::new(),
    };
    upsert_runtime_graph_artifact(
        &store,
        &project_id,
        &registry_key,
        artifact,
        Some(snapshot_path.to_string()),
        env.get(GRAPH_QUERY_ENV_CONSTITUTION_PATH).cloned(),
        metadata,
    )
}

fn load_snapshot_from_path(
    env: &HashMap<String, String>,
) -> Result<(String, RuntimeGraphSnapshotArtifact), NativeToolEngineError> {
    let snapshot_path = env
        .get(GRAPH_QUERY_ENV_SNAPSHOT_PATH)
        .ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_missing",
                format!("missing runtime env {GRAPH_QUERY_ENV_SNAPSHOT_PATH}"),
            )
        })?
        .clone();
    let raw = fs::read_to_string(&snapshot_path).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("failed to read graph snapshot '{snapshot_path}': {error}"),
        )
    })?;
    if raw.trim().is_empty() || raw.trim() == "{}" {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("graph snapshot '{snapshot_path}' is empty"),
        ));
    }
    let artifact = serde_json::from_str::<RuntimeGraphSnapshotArtifact>(&raw).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid graph snapshot '{snapshot_path}': {error}"),
        )
    })?;
    Ok((snapshot_path, artifact))
}

fn ensure_authoritative_runtime_graph(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    env: &HashMap<String, String>,
    project_id: &str,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    let authoritative_key = authoritative_registry_key(project_id);
    if let Some(record) = store
        .graphcode_artifact_by_registry_key(&authoritative_key)
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?
    {
        return load_runtime_graph_from_record(&record, project_id);
    }
    let (snapshot_path, artifact) = load_snapshot_from_path(env)?;
    sync_snapshot_into_runtime_registry(env, &snapshot_path, &artifact)?;
    Ok(artifact)
}

fn build_graph_query_index(
    artifact: &RuntimeGraphSnapshotArtifact,
    constitution_path: Option<&Path>,
) -> Result<GraphQueryIndex, NativeToolEngineError> {
    let partition_paths =
        constitution_path.map_or_else(BTreeMap::new, load_partition_paths_from_constitution);
    let repositories = artifact
        .repositories
        .iter()
        .map(|repo| GraphQueryRepository {
            repo_name: repo.repo_name.clone(),
            document: repo.document.clone(),
        })
        .collect::<Vec<_>>();
    GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
        .map_err(|error| NativeToolEngineError::new(error.code, error.message, false))
}

fn compute_session_impact_analysis(
    old_artifact: &RuntimeGraphSnapshotArtifact,
    new_artifact: &RuntimeGraphSnapshotArtifact,
    constitution_path: Option<&Path>,
) -> Result<SessionImpactAnalysis, NativeToolEngineError> {
    let old_index = build_graph_query_index(old_artifact, constitution_path)?;
    let new_index = build_graph_query_index(new_artifact, constitution_path)?;
    let node_ids = old_index
        .nodes
        .keys()
        .chain(new_index.nodes.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut analysis = SessionImpactAnalysis {
        current_node_ids: new_index.nodes.keys().cloned().collect(),
        ..SessionImpactAnalysis::default()
    };
    for node_id in node_ids {
        let old_node = old_index.nodes.get(&node_id);
        let new_node = new_index.nodes.get(&node_id);
        let changed = match (old_node, new_node) {
            (Some(old_node), Some(new_node)) => {
                old_node.repo_name != new_node.repo_name
                    || old_node.logical_key != new_node.logical_key
                    || old_node.node_class != new_node.node_class
                    || old_node.path != new_node.path
                    || old_node.partition != new_node.partition
                    || old_index.outgoing.get(&node_id) != new_index.outgoing.get(&node_id)
            }
            _ => true,
        };
        if !changed {
            continue;
        }
        analysis.changed_node_ids.insert(node_id.clone());
        if let Some(node) = old_node.or(new_node) {
            if !node.logical_key.is_empty() {
                analysis
                    .changed_logical_keys
                    .insert(node.logical_key.clone());
            }
            if let Some(path) = node.path.as_ref() {
                analysis.changed_paths.insert(path.clone());
            }
        }
    }
    Ok(analysis)
}

fn parse_node_refs(raw: &str) -> Vec<PersistedGraphQueryNodeRef> {
    serde_json::from_str::<Vec<PersistedGraphQueryNodeRef>>(raw).unwrap_or_else(|_| {
        serde_json::from_str::<Vec<String>>(raw)
            .unwrap_or_default()
            .into_iter()
            .map(PersistedGraphQueryNodeRef::from_node_id)
            .collect()
    })
}

fn parse_focus_ids(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

fn parse_traversals(raw: &str) -> Vec<PersistedGraphQueryTraversal> {
    serde_json::from_str::<Vec<PersistedGraphQueryTraversal>>(raw).unwrap_or_default()
}

fn node_ref_is_current(
    node_ref: &PersistedGraphQueryNodeRef,
    analysis: &SessionImpactAnalysis,
) -> bool {
    analysis.current_node_ids.contains(&node_ref.node_id)
        && !analysis.changed_node_ids.contains(&node_ref.node_id)
        && (node_ref.logical_key.is_empty()
            || !analysis
                .changed_logical_keys
                .contains(&node_ref.logical_key))
        && node_ref
            .path
            .as_ref()
            .is_none_or(|path| !analysis.changed_paths.contains(path))
}

fn traversal_is_current(
    traversal: &PersistedGraphQueryTraversal,
    analysis: &SessionImpactAnalysis,
) -> bool {
    if traversal.node_ids.is_empty()
        && traversal.logical_keys.is_empty()
        && traversal.paths.is_empty()
    {
        return analysis.changed_node_ids.is_empty() && analysis.changed_paths.is_empty();
    }
    !traversal
        .node_ids
        .iter()
        .any(|node_id| analysis.changed_node_ids.contains(node_id))
        && !traversal
            .logical_keys
            .iter()
            .any(|logical_key| analysis.changed_logical_keys.contains(logical_key))
        && !traversal
            .paths
            .iter()
            .any(|path| analysis.changed_paths.contains(path))
}

fn selectively_invalidate_session(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    registry_key: &str,
    old_artifact: &RuntimeGraphSnapshotArtifact,
    new_artifact: &RuntimeGraphSnapshotArtifact,
    constitution_path: Option<&Path>,
) -> Result<(), NativeToolEngineError> {
    let session_ref = active_session_ref(registry_key);
    let Some(session) = store
        .graphcode_session_by_ref(&session_ref)
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?
    else {
        return ensure_graphcode_session_exists(
            store,
            registry_key,
            &new_artifact.canonical_fingerprint,
        );
    };
    if old_artifact.canonical_fingerprint == new_artifact.canonical_fingerprint {
        return store
            .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
                session_ref,
                registry_key: registry_key.to_string(),
                substrate_kind: session.substrate_kind,
                current_focus_json: session.current_focus_json,
                pinned_nodes_json: session.pinned_nodes_json,
                recent_traversals_json: session.recent_traversals_json,
                working_set_refs_json: session.working_set_refs_json,
                hydrated_excerpts_json: session.hydrated_excerpts_json,
                path_artifacts_json: session.path_artifacts_json,
                snapshot_fingerprint: new_artifact.canonical_fingerprint.clone(),
                freshness_state: "current".to_string(),
            })
            .map_err(|error| {
                graph_query_runtime_error_with_refresh_hint(&error.code, error.message)
            });
    }
    let analysis = compute_session_impact_analysis(old_artifact, new_artifact, constitution_path)?;
    let current_focus = parse_focus_ids(&session.current_focus_json)
        .into_iter()
        .filter(|node_id| {
            analysis.current_node_ids.contains(node_id)
                && !analysis.changed_node_ids.contains(node_id)
        })
        .collect::<Vec<_>>();
    let pinned_nodes = parse_node_refs(&session.pinned_nodes_json)
        .into_iter()
        .filter(|node_ref| node_ref_is_current(node_ref, &analysis))
        .collect::<Vec<_>>();
    let working_set = parse_node_refs(&session.working_set_refs_json)
        .into_iter()
        .filter(|node_ref| node_ref_is_current(node_ref, &analysis))
        .collect::<Vec<_>>();
    let hydrated = parse_node_refs(&session.hydrated_excerpts_json)
        .into_iter()
        .filter(|node_ref| node_ref_is_current(node_ref, &analysis))
        .collect::<Vec<_>>();
    let traversals = parse_traversals(&session.recent_traversals_json)
        .into_iter()
        .filter(|traversal| traversal_is_current(traversal, &analysis))
        .collect::<Vec<_>>();
    let path_artifacts = serde_json::from_str::<Vec<GraphQueryEdge>>(&session.path_artifacts_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|edge| {
            analysis.current_node_ids.contains(&edge.source)
                && analysis.current_node_ids.contains(&edge.target)
                && !analysis.changed_node_ids.contains(&edge.source)
                && !analysis.changed_node_ids.contains(&edge.target)
        })
        .collect::<Vec<_>>();
    store
        .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
            session_ref,
            registry_key: registry_key.to_string(),
            substrate_kind: session.substrate_kind,
            current_focus_json: serde_json::to_string(&current_focus)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            pinned_nodes_json: serde_json::to_string(&pinned_nodes)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            recent_traversals_json: serde_json::to_string(&traversals)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            working_set_refs_json: serde_json::to_string(&working_set)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            hydrated_excerpts_json: serde_json::to_string(&hydrated)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            path_artifacts_json: serde_json::to_string(&path_artifacts)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            snapshot_fingerprint: new_artifact.canonical_fingerprint.clone(),
            freshness_state: "current".to_string(),
        })
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))
}

fn build_incremental_execution_artifact(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    registry_key: &str,
    env: &HashMap<String, String>,
    base_artifact: &RuntimeGraphSnapshotArtifact,
) -> Result<
    (
        RuntimeGraphSnapshotArtifact,
        Vec<StoredRuntimeGraphIncrementalRepository>,
    ),
    NativeToolEngineError,
> {
    let mut repositories = Vec::new();
    let mut incremental_repos = Vec::new();
    for repo in &base_artifact.repositories {
        let repo_root = resolve_repo_worktree_path(&repo.repo_name, &repo.repo_path, env);
        let commit_hash = resolve_repo_head_commit(&repo_root, env)?;
        let state_file =
            runtime_graph_incremental_state_file(&store.db_path, registry_key, &repo.repo_name);
        let build = build_code_graph_incremental(&CodeGraphIncrementalBuildInput {
            build: CodeGraphBuildInput {
                repository_path: repo_root.clone(),
                commit_hash: commit_hash.clone(),
                config: CodeGraphExtractorConfig::default(),
            },
            state_file: state_file.clone(),
        })
        .map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_rebuild_failed",
                format!(
                    "failed to rebuild execution graph for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        repositories.push(RuntimeGraphSnapshotRepository {
            repo_name: repo.repo_name.clone(),
            repo_path: repo_root.to_string_lossy().to_string(),
            commit_hash,
            canonical_fingerprint: build.canonical_fingerprint.clone(),
            document: PortableDocument::from_document(&build.document),
        });
        incremental_repos.push(StoredRuntimeGraphIncrementalRepository {
            repo_name: repo.repo_name.clone(),
            state_file: state_file.to_string_lossy().to_string(),
            stats: build.incremental,
        });
    }
    let provenance = RuntimeGraphSnapshotProvenance {
        head_commits: repositories
            .iter()
            .map(|repo| RuntimeGraphSnapshotCommit {
                repo_name: repo.repo_name.clone(),
                repo_path: repo.repo_path.clone(),
                commit_hash: repo.commit_hash.clone(),
            })
            .collect(),
    };
    let canonical_fingerprint = aggregate_snapshot_fingerprint_registry_style(&repositories);
    Ok((
        RuntimeGraphSnapshotArtifact {
            profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            canonical_fingerprint,
            provenance,
            repositories,
        },
        incremental_repos,
    ))
}

fn ensure_execution_runtime_graph_current(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    env: &HashMap<String, String>,
    project_id: &str,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    let registry_key = active_runtime_registry_key(project_id, env);
    let existing_record = store
        .graphcode_artifact_by_registry_key(&registry_key)
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?;
    if let Some(record) = existing_record.as_ref() {
        if record.freshness_state == "current" {
            return load_runtime_graph_from_record(record, &registry_key);
        }
    }
    let base_artifact = ensure_authoritative_runtime_graph(store, env, project_id)?;
    let old_artifact = existing_record
        .as_ref()
        .map(|record| load_runtime_graph_from_record(record, &registry_key))
        .transpose()?;
    let (artifact, incremental_repos) =
        build_incremental_execution_artifact(store, &registry_key, env, &base_artifact)?;
    let metadata = StoredRuntimeGraphSnapshotMetadata {
        version: 2,
        profile_version: artifact.profile_version.clone(),
        canonical_fingerprint: artifact.canonical_fingerprint.clone(),
        provenance: artifact.provenance.clone(),
        graph_scope: GRAPH_QUERY_EXECUTION_SCOPE.to_string(),
        dirty_paths: Vec::new(),
        refresh_reason: Some(existing_record.as_ref().map_or_else(
            || "initial_execution_build".to_string(),
            |_| "incremental_refresh".to_string(),
        )),
        incremental_repos,
    };
    upsert_runtime_graph_artifact(
        store,
        project_id,
        &registry_key,
        &artifact,
        env.get(GRAPH_QUERY_ENV_SNAPSHOT_PATH).cloned(),
        env.get(GRAPH_QUERY_ENV_CONSTITUTION_PATH).cloned(),
        metadata,
    )?;
    selectively_invalidate_session(
        store,
        &registry_key,
        old_artifact.as_ref().unwrap_or(&base_artifact),
        &artifact,
        env.get(GRAPH_QUERY_ENV_CONSTITUTION_PATH)
            .map(PathBuf::from)
            .as_deref(),
    )?;
    Ok(artifact)
}

fn mark_registry_graph_dirty(
    store: &crate::native::runtime_hardening::NativeRuntimeStateStore,
    registry_key: &str,
    dirty_paths: &[PathBuf],
) -> Result<(), NativeToolEngineError> {
    let Some(record) = store
        .graphcode_artifact_by_registry_key(registry_key)
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?
    else {
        return Ok(());
    };
    let Some(mut metadata) = parse_snapshot_metadata(&record.snapshot_json) else {
        store
            .mark_graphcode_artifact_freshness_by_registry_key(registry_key, "stale")
            .map_err(|error| {
                graph_query_runtime_error_with_refresh_hint(&error.code, error.message)
            })?;
        store
            .mark_graphcode_session_freshness_by_registry_key(registry_key, "stale")
            .map_err(|error| {
                graph_query_runtime_error_with_refresh_hint(&error.code, error.message)
            })?;
        return Ok(());
    };
    let mut dirty = metadata.dirty_paths.into_iter().collect::<BTreeSet<_>>();
    dirty.extend(
        dirty_paths
            .iter()
            .map(|path| relative_display(path))
            .filter(|path| !path.is_empty()),
    );
    metadata.dirty_paths = dirty.into_iter().collect();
    metadata.refresh_reason = Some("tool_mutation".to_string());
    let snapshot_json = serialize_snapshot_metadata(&metadata)?;
    store
        .upsert_graphcode_artifact(&crate::native::runtime_hardening::GraphCodeArtifactUpsert {
            registry_key: record.registry_key,
            project_id: record.project_id,
            substrate_kind: record.substrate_kind,
            storage_backend: record.storage_backend,
            storage_reference: record.storage_reference,
            derivative_snapshot_path: record.derivative_snapshot_path,
            constitution_path: record.constitution_path,
            canonical_fingerprint: record.canonical_fingerprint,
            profile_version: record.profile_version,
            ucp_engine_version: record.ucp_engine_version,
            extractor_version: record.extractor_version,
            runtime_version: record.runtime_version,
            freshness_state: "stale".to_string(),
            repo_manifest_json: record.repo_manifest_json,
            active_session_ref: record.active_session_ref,
            snapshot_json,
        })
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))?;
    store
        .mark_graphcode_session_freshness_by_registry_key(registry_key, "stale")
        .map_err(|error| graph_query_runtime_error_with_refresh_hint(&error.code, error.message))
}

pub(crate) fn mark_runtime_graph_dirty(env: &HashMap<String, String>, dirty_paths: &[PathBuf]) {
    let Some(project_id) = env.get(GRAPH_QUERY_ENV_PROJECT_ID) else {
        return;
    };
    let Ok(store) = open_runtime_state_store(env) else {
        return;
    };
    let registry_key = active_runtime_registry_key(project_id, env);
    let _ = mark_registry_graph_dirty(&store, &registry_key, dirty_paths);
}

pub(super) fn mark_runtime_graph_snapshot_freshness(
    env: &HashMap<String, String>,
    freshness_state: &str,
) {
    let Some(project_id) = env.get(GRAPH_QUERY_ENV_PROJECT_ID) else {
        return;
    };
    let Ok(store) = open_runtime_state_store(env) else {
        return;
    };
    let registry_key = active_runtime_registry_key(project_id, env);
    let _ = store.mark_graphcode_artifact_freshness_by_registry_key(&registry_key, freshness_state);
    let _ = store.mark_graphcode_session_freshness_by_registry_key(&registry_key, freshness_state);
}

pub(crate) fn load_runtime_graph_snapshot(
    env: &HashMap<String, String>,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    if let Some(raw) = env.get(GRAPH_QUERY_ENV_GATE_ERROR) {
        let gate = serde_json::from_str::<RuntimeGraphQueryGateError>(raw).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to decode {GRAPH_QUERY_ENV_GATE_ERROR}: {error}"
            ))
        })?;
        let message = gate.hint.as_ref().map_or_else(
            || gate.message.clone(),
            |hint| format!("{}. Hint: {hint}", gate.message),
        );
        return Err(graph_query_runtime_error(&gate.code, message));
    }
    if let Some(project_id) = env.get(GRAPH_QUERY_ENV_PROJECT_ID) {
        let store = open_runtime_state_store(env)?;
        if is_attempt_scoped_registry(project_id, env) {
            return ensure_execution_runtime_graph_current(&store, env, project_id);
        }
        let registry_key = authoritative_registry_key(project_id);
        if let Some(record) = store
            .graphcode_artifact_by_registry_key(&registry_key)
            .map_err(|error| {
                graph_query_runtime_error_with_refresh_hint(&error.code, error.message)
            })?
        {
            return load_runtime_graph_from_record(&record, project_id);
        }
    }
    let (snapshot_path, artifact) = load_snapshot_from_path(env)?;
    sync_snapshot_into_runtime_registry(env, &snapshot_path, &artifact)?;
    Ok(artifact)
}

pub(super) fn validate_runtime_graph_snapshot(
    artifact: &RuntimeGraphSnapshotArtifact,
    env: &HashMap<String, String>,
) -> Result<(), NativeToolEngineError> {
    if artifact.profile_version != CODEGRAPH_PROFILE_MARKER {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_profile_mismatch",
            format!(
                "snapshot profile '{}' is not supported; expected '{}'",
                artifact.profile_version, CODEGRAPH_PROFILE_MARKER
            ),
        ));
    }
    let head_by_repo: HashMap<(String, String), String> = artifact
        .provenance
        .head_commits
        .iter()
        .map(|entry| {
            (
                (entry.repo_name.clone(), entry.repo_path.clone()),
                entry.commit_hash.clone(),
            )
        })
        .collect();
    for repo in &artifact.repositories {
        let document = repo.document.to_document().map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "invalid portable document for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        let computed = canonical_fingerprint(&document).map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "failed to compute fingerprint for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        if computed != repo.canonical_fingerprint {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!("fingerprint mismatch for repository '{}'", repo.repo_name),
            ));
        }
        let key = (repo.repo_name.clone(), repo.repo_path.clone());
        let recorded_head = head_by_repo.get(&key).ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "missing head commit provenance for repository '{}'",
                    repo.repo_name
                ),
            )
        })?;
        if recorded_head != &repo.commit_hash {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "snapshot repository commit mismatch for '{}' (snapshot={} provenance={})",
                    repo.repo_name, repo.commit_hash, recorded_head
                ),
            ));
        }
        let current_head = resolve_repo_head_commit(Path::new(&repo.repo_path), env)?;
        if current_head != *recorded_head {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "graph snapshot is stale for repository '{}' (snapshot={} current={})",
                    repo.repo_name, recorded_head, current_head
                ),
            ));
        }
    }
    let aggregate = aggregate_snapshot_fingerprint_registry_style(&artifact.repositories);
    if aggregate != artifact.canonical_fingerprint {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_integrity_invalid",
            "aggregate graph snapshot fingerprint mismatch",
        ));
    }
    Ok(())
}

pub(crate) fn aggregate_snapshot_fingerprint_registry_style(
    repositories: &[RuntimeGraphSnapshotRepository],
) -> String {
    let mut entries = repositories
        .iter()
        .map(|repo| {
            format!(
                "{}|{}|{}|{}",
                repo.repo_name, repo.repo_path, repo.commit_hash, repo.canonical_fingerprint
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    let joined = entries.join("\n");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    joined.as_bytes().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
