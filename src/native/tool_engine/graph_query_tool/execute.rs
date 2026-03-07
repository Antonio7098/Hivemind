use super::env::graph_query_bounds_from_env;
use super::snapshot::{load_runtime_graph_snapshot, validate_runtime_graph_snapshot};
use super::*;

fn map_graph_query_error(
    error: crate::core::graph_query::GraphQueryError,
) -> NativeToolEngineError {
    NativeToolEngineError::new(error.code, error.message, false)
}
pub(crate) fn handle_graph_query(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GraphQueryInput>(input)?;
    let request = input.into_request()?;
    let bounds = graph_query_bounds_from_env(ctx.env)?;
    let artifact = load_runtime_graph_snapshot(ctx.env)?;
    validate_runtime_graph_snapshot(&artifact, ctx.env)?;

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

    let started = Instant::now();
    let mut result = index
        .execute(&request, &artifact.canonical_fingerprint, &bounds)
        .map_err(map_graph_query_error)?;
    result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode graph_query output: {error}"))
    })
}
