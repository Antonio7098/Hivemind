use super::errors::graph_query_runtime_error_with_refresh_hint;
use super::*;

pub(super) fn resolve_repo_head_commit(
    repo_path: &Path,
    env: &HashMap<String, String>,
) -> Result<String, NativeToolEngineError> {
    let mut cmd = Command::new("git");
    cmd.args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .env_clear();
    for (key, value) in deterministic_env_pairs(env) {
        cmd.env(key, value);
    }
    let output = cmd.output().map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!(
                "failed to resolve git HEAD for '{}': {error}",
                repo_path.display()
            ),
        )
    })?;
    if !output.status.success() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!(
                "failed to resolve git HEAD for '{}': {}",
                repo_path.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!("resolved empty HEAD for '{}'", repo_path.display()),
        ));
    }
    Ok(head)
}
pub(super) fn graph_query_bounds_from_env(
    env: &HashMap<String, String>,
) -> Result<GraphQueryBounds, NativeToolEngineError> {
    let mut bounds = GraphQueryBounds::default();
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT") {
        bounds.max_results_limit = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH") {
        bounds.max_subgraph_depth = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS") {
        bounds.default_max_results = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS '{raw}': {error}"
            ))
        })?;
    }
    if bounds.max_results_limit == 0
        || bounds.max_subgraph_depth == 0
        || bounds.default_max_results == 0
    {
        return Err(NativeToolEngineError::execution(
            "graph query bounds configuration values must be > 0",
        ));
    }
    if bounds.default_max_results > bounds.max_results_limit {
        return Err(NativeToolEngineError::execution(format!(
            "graph query default max results {} exceeds max limit {}",
            bounds.default_max_results, bounds.max_results_limit
        )));
    }
    Ok(bounds)
}
