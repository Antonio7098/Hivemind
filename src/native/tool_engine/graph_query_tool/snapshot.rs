use super::env::resolve_repo_head_commit;
use super::errors::{graph_query_runtime_error, graph_query_runtime_error_with_refresh_hint};
use super::types::RuntimeGraphQueryGateError;
use super::*;

pub(super) fn load_runtime_graph_snapshot(
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

    serde_json::from_str::<RuntimeGraphSnapshotArtifact>(&raw).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid graph snapshot '{snapshot_path}': {error}"),
        )
    })
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
