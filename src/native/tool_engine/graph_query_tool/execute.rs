use super::env::graph_query_bounds_from_env;
use super::python_query::execute_python_graph_query;
use super::snapshot::{
    load_runtime_graph_snapshot, mark_runtime_graph_snapshot_freshness,
    validate_runtime_graph_snapshot,
};
use super::types::{
    GraphQueryToolRequest, PersistedGraphQueryNodeRef, PersistedGraphQueryTraversal,
};
use super::*;
use std::collections::BTreeSet;

const HIVEMIND_ATTEMPT_ID_ENV: &str = "HIVEMIND_ATTEMPT_ID";

fn active_graph_registry_key(project_id: &str, env: &HashMap<String, String>) -> String {
    env.get(HIVEMIND_ATTEMPT_ID_ENV)
        .filter(|value| !value.trim().is_empty())
        .map(|attempt_id| format!("{project_id}:codegraph:attempt:{attempt_id}"))
        .unwrap_or_else(|| format!("{project_id}:codegraph"))
}

fn map_graph_query_error(
    error: crate::core::graph_query::GraphQueryError,
) -> NativeToolEngineError {
    NativeToolEngineError::new(error.code, error.message, false)
}

fn persist_graph_query_session(
    ctx: &ToolExecutionContext<'_>,
    result: &GraphQueryResult,
) -> Result<(), NativeToolEngineError> {
    let Some(project_id) = ctx.env.get(GRAPH_QUERY_ENV_PROJECT_ID).cloned() else {
        return Ok(());
    };
    let store = crate::native::runtime_hardening::NativeRuntimeStateStore::open(
        &crate::native::runtime_hardening::RuntimeHardeningConfig::from_env(ctx.env),
    )
    .map_err(|error| NativeToolEngineError::execution(error.message))?;
    let registry_key = active_graph_registry_key(&project_id, ctx.env);
    let focus = serde_json::to_string(
        &result
            .nodes
            .iter()
            .take(12)
            .map(|node| node.node_id.clone())
            .collect::<Vec<_>>(),
    )
    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    let working_set = result
        .nodes
        .iter()
        .take(12)
        .map(PersistedGraphQueryNodeRef::from_graph_node)
        .collect::<Vec<_>>();
    let traversals = serde_json::to_string(&vec![PersistedGraphQueryTraversal {
        query_kind: result.query_kind.clone(),
        max_results: result.max_results,
        truncated: result.truncated,
        duration_ms: result.duration_ms,
        node_ids: result
            .nodes
            .iter()
            .map(|node| node.node_id.clone())
            .collect(),
        logical_keys: result
            .nodes
            .iter()
            .map(|node| node.logical_key.clone())
            .collect(),
        paths: result
            .nodes
            .iter()
            .filter_map(|node| node.path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    }])
    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    let excerpts = serde_json::to_string(
        &result
            .nodes
            .iter()
            .take(8)
            .map(PersistedGraphQueryNodeRef::from_graph_node)
            .collect::<Vec<_>>(),
    )
    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    let path_artifacts = serde_json::to_string(
        &result
            .edges
            .iter()
            .take(12)
            .map(|edge| {
                serde_json::json!({
                    "source": edge.source,
                    "target": edge.target,
                    "edge_type": edge.edge_type,
                })
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    store
        .upsert_graphcode_session(&crate::native::runtime_hardening::GraphCodeSessionUpsert {
            session_ref: format!("{registry_key}:session:default"),
            registry_key,
            substrate_kind: "codegraph".to_string(),
            current_focus_json: focus.clone(),
            pinned_nodes_json: "[]".to_string(),
            recent_traversals_json: traversals,
            working_set_refs_json: serde_json::to_string(&working_set)
                .map_err(|error| NativeToolEngineError::execution(error.to_string()))?,
            hydrated_excerpts_json: excerpts,
            path_artifacts_json: path_artifacts,
            snapshot_fingerprint: result.canonical_fingerprint.clone(),
            freshness_state: "current".to_string(),
        })
        .map_err(|error| NativeToolEngineError::execution(error.message))
}

pub(crate) fn handle_graph_query(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GraphQueryInput>(input)?;
    let request = input.into_request()?;
    let bounds = graph_query_bounds_from_env(ctx.env)?;
    let artifact = load_runtime_graph_snapshot(ctx.env)?;
    if let Err(error) = validate_runtime_graph_snapshot(&artifact, ctx.env) {
        if matches!(
            error.code.as_str(),
            "graph_snapshot_stale" | "graph_snapshot_integrity_invalid"
        ) {
            mark_runtime_graph_snapshot_freshness(ctx.env, "stale");
        }
        return Err(error);
    }
    mark_runtime_graph_snapshot_freshness(ctx.env, "current");

    let partition_paths = ctx
        .env
        .get(GRAPH_QUERY_ENV_CONSTITUTION_PATH)
        .map(PathBuf::from)
        .as_deref()
        .map_or_else(BTreeMap::new, load_partition_paths_from_constitution);

    let repositories = artifact
        .repositories
        .iter()
        .map(|repo| GraphQueryRepository {
            repo_name: repo.repo_name.clone(),
            document: repo.document.clone(),
        })
        .collect::<Vec<_>>();
    let index = GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
        .map_err(map_graph_query_error)?;

    let result = match request {
        GraphQueryToolRequest::Native(request) => {
            let started = Instant::now();
            let mut result = index
                .execute(&request, &artifact.canonical_fingerprint, &bounds)
                .map_err(map_graph_query_error)?;
            result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            result
        }
        GraphQueryToolRequest::Python(input) => {
            execute_python_graph_query(ctx, &input, &artifact, &index, &bounds, timeout_ms)?
        }
    };
    persist_graph_query_session(ctx, &result)?;
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode graph_query output: {error}"))
    })
}
