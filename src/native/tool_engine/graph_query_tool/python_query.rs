use super::types::GraphQueryPythonInput;
use super::*;
use crate::core::graph_query::{GraphQueryCost, GraphQueryEdge, GraphQueryNode};
use std::collections::BTreeSet;

const GRAPH_QUERY_PYTHON_BIN_ENV: &str = "HIVEMIND_GRAPH_QUERY_PYTHON";
const PYTHON_QUERY_STDOUT_SUMMARY_LIMIT: usize = 512;
const PYTHON_QUERY_DEFAULT_MAX_SECONDS: f64 = 3.0;
const PYTHON_QUERY_DEFAULT_MAX_OPERATIONS: usize = 48;
const PYTHON_QUERY_DEFAULT_MAX_TRACE_EVENTS: usize = 4_000;
const PYTHON_QUERY_DEFAULT_MAX_STDOUT_CHARS: usize = 2_000;

#[derive(Debug, Serialize)]
struct PythonGraphQueryRequestPayload<'a> {
    portable_document_json: &'a str,
    code: &'a str,
    bindings: &'a BTreeMap<String, Value>,
    include_export: bool,
    export_kwargs: &'a BTreeMap<String, Value>,
    limits: Value,
}

pub(super) fn execute_python_graph_query(
    ctx: &ToolExecutionContext<'_>,
    input: &GraphQueryPythonInput,
    artifact: &RuntimeGraphSnapshotArtifact,
    index: &GraphQueryIndex,
    bounds: &GraphQueryBounds,
    timeout_ms: u64,
) -> Result<GraphQueryResult, NativeToolEngineError> {
    let (repo, repository_index) = resolve_python_query_repository(artifact, input)?;
    let portable_document_json = serde_json::to_string(&repo.document)
        .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    let python_bin = resolve_python_query_python_bin(ctx)?;
    let max_results = bounded_python_query_max_results(input.max_results, bounds)?;
    let include_export = input.include_export.unwrap_or(true);

    let mut bindings = BTreeMap::from([
        (
            "repo_name".to_string(),
            Value::String(repo.repo_name.clone()),
        ),
        (
            "repo_path".to_string(),
            Value::String(repo.repo_path.clone()),
        ),
        (
            "canonical_fingerprint".to_string(),
            Value::String(repo.canonical_fingerprint.clone()),
        ),
        (
            "worktree_root".to_string(),
            Value::String(ctx.worktree.to_string_lossy().to_string()),
        ),
    ]);
    bindings.extend(input.bindings.clone());

    let mut export_kwargs = BTreeMap::from([
        ("compact".to_string(), Value::Bool(true)),
        ("max_frontier_actions".to_string(), Value::from(6_u64)),
    ]);
    export_kwargs.extend(input.export_kwargs.clone());
    let limits = serde_json::json!({
        "max_seconds": input
            .limits
            .as_ref()
            .and_then(|limits| limits.max_seconds)
            .unwrap_or(PYTHON_QUERY_DEFAULT_MAX_SECONDS),
        "max_operations": input
            .limits
            .as_ref()
            .and_then(|limits| limits.max_operations)
            .unwrap_or(PYTHON_QUERY_DEFAULT_MAX_OPERATIONS),
        "max_trace_events": input
            .limits
            .as_ref()
            .and_then(|limits| limits.max_trace_events)
            .unwrap_or(PYTHON_QUERY_DEFAULT_MAX_TRACE_EVENTS),
        "max_stdout_chars": input
            .limits
            .as_ref()
            .and_then(|limits| limits.max_stdout_chars)
            .unwrap_or(PYTHON_QUERY_DEFAULT_MAX_STDOUT_CHARS),
    });
    let payload = PythonGraphQueryRequestPayload {
        portable_document_json: &portable_document_json,
        code: &input.code,
        bindings: &bindings,
        include_export,
        export_kwargs: &export_kwargs,
        limits,
    };
    let request_bytes = serde_json::to_vec(&payload)
        .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;

    let started = Instant::now();
    let response = run_python_graph_query_subprocess(&python_bin, &request_bytes, timeout_ms)?;
    let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    build_python_query_result(
        &response,
        repo,
        repository_index,
        index,
        &artifact.canonical_fingerprint,
        max_results,
        duration_ms,
    )
}

fn resolve_python_query_repository<'a>(
    artifact: &'a RuntimeGraphSnapshotArtifact,
    input: &GraphQueryPythonInput,
) -> Result<(&'a RuntimeGraphSnapshotRepository, usize), NativeToolEngineError> {
    if let Some(repo_name) = input.repo_name.as_deref() {
        return artifact
            .repositories
            .iter()
            .enumerate()
            .find(|(_, repo)| repo.repo_name == repo_name)
            .map(|(index, repo)| (repo, index))
            .ok_or_else(|| {
                NativeToolEngineError::validation(format!(
                    "graph_query.python_query repo_name '{repo_name}' was not found"
                ))
            });
    }
    if artifact.repositories.len() == 1 {
        return Ok((&artifact.repositories[0], 0));
    }
    Err(NativeToolEngineError::validation(format!(
        "graph_query.python_query requires 'repo_name' when multiple repositories are attached: {}",
        artifact
            .repositories
            .iter()
            .map(|repo| repo.repo_name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

fn resolve_python_query_python_bin(
    ctx: &ToolExecutionContext<'_>,
) -> Result<String, NativeToolEngineError> {
    let candidate = ctx
        .env
        .get(GRAPH_QUERY_PYTHON_BIN_ENV)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| "python3".to_string());
    if candidate.contains('/') && !Path::new(&candidate).is_file() {
        return Err(NativeToolEngineError::new(
            "graph_query_python_unavailable",
            format!(
                "graph_query.python_query could not find Python executable '{}'. Set {} to a Python interpreter with the local 'ucp' package installed.",
                candidate, GRAPH_QUERY_PYTHON_BIN_ENV
            ),
            false,
        ));
    }
    Ok(candidate)
}

fn bounded_python_query_max_results(
    raw: Option<usize>,
    bounds: &GraphQueryBounds,
) -> Result<usize, NativeToolEngineError> {
    let value = raw.unwrap_or(bounds.default_max_results);
    if value == 0 {
        return Err(NativeToolEngineError::validation(
            "graph_query.python_query max_results must be > 0",
        ));
    }
    if value > bounds.max_results_limit {
        return Err(NativeToolEngineError::validation(format!(
            "graph_query.python_query max_results {} exceeds limit {}",
            value, bounds.max_results_limit
        )));
    }
    Ok(value)
}

fn run_python_graph_query_subprocess(
    python_bin: &str,
    request_bytes: &[u8],
    timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let mut child = Command::new(python_bin)
        .arg("-c")
        .arg(PYTHON_GRAPH_QUERY_BOOTSTRAP)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            NativeToolEngineError::new(
                "graph_query_python_unavailable",
                format!(
                    "failed to launch graph_query.python_query interpreter '{}': {error}",
                    python_bin
                ),
                false,
            )
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request_bytes)
            .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
    }
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| NativeToolEngineError::execution(error.to_string()))?
        {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut handle) = child.stdout.take() {
                handle
                    .read_to_string(&mut stdout)
                    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
            }
            if let Some(mut handle) = child.stderr.take() {
                handle
                    .read_to_string(&mut stderr)
                    .map_err(|error| NativeToolEngineError::execution(error.to_string()))?;
            }
            let payload = serde_json::from_str::<Value>(&stdout).map_err(|error| {
                NativeToolEngineError::new(
                    "graph_query_python_invalid_output",
                    format!(
                        "graph_query.python_query returned non-JSON output: {error}; stderr={}",
                        stderr.trim()
                    ),
                    false,
                )
            })?;
            if !status.success() && payload.get("ok").and_then(Value::as_bool) != Some(false) {
                return Err(NativeToolEngineError::new(
                    "graph_query_python_failed",
                    format!(
                        "graph_query.python_query interpreter exited with status {status}; stderr={}",
                        stderr.trim()
                    ),
                    false,
                ));
            }
            return Ok(payload);
        }
        if started.elapsed() > Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeToolEngineError::timeout("graph_query", timeout_ms));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn build_python_query_result(
    response: &Value,
    repo: &RuntimeGraphSnapshotRepository,
    repository_index: usize,
    index: &GraphQueryIndex,
    canonical_fingerprint: &str,
    max_results: usize,
    duration_ms: u64,
) -> Result<GraphQueryResult, NativeToolEngineError> {
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let error_type = response
            .get("error")
            .and_then(|error| error.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("graph_query_python_failed");
        let error_message = response
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("graph_query.python_query failed");
        let traceback = response
            .get("error")
            .and_then(|error| error.get("traceback"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Err(NativeToolEngineError::new(
            if error_type == "QueryLimitExceededError" {
                "graph_query_python_limits_exceeded"
            } else {
                "graph_query_python_failed"
            },
            if traceback.trim().is_empty() {
                error_message.to_string()
            } else {
                format!("{error_message}\n{traceback}")
            },
            false,
        ));
    }

    let export = response.get("export");
    let block_to_node = &index.repositories[repository_index].block_to_node_id;
    let exported_nodes = export
        .and_then(|value| value.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let exported_edges = export
        .and_then(|value| value.get("edges"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected_block_ids = response
        .get("selected_block_ids")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    let mut node_ids = Vec::new();
    if exported_nodes.is_empty() {
        for block_id in &selected_block_ids {
            if let Some(node_id) = block_to_node.get(block_id) {
                node_ids.push(node_id.clone());
            }
        }
    } else {
        for node in &exported_nodes {
            if let Some(node_id) = node
                .get("block_id")
                .and_then(Value::as_str)
                .and_then(|block_id| block_to_node.get(block_id))
            {
                node_ids.push(node_id.clone());
            }
        }
    }
    let truncated = node_ids.len() > max_results;
    node_ids.truncate(max_results);
    let selected = node_ids.iter().cloned().collect::<BTreeSet<_>>();
    let nodes = node_ids
        .iter()
        .filter_map(|node_id| {
            index.nodes.get(node_id).map(|node| GraphQueryNode {
                node_id: node_id.clone(),
                repo_name: node.repo_name.clone(),
                logical_key: node.logical_key.clone(),
                node_class: node.node_class.clone(),
                path: node.path.clone(),
                partition: node.partition.clone(),
            })
        })
        .collect::<Vec<_>>();
    let edges = if exported_edges.is_empty() {
        index
            .all_edges
            .iter()
            .filter(|edge| selected.contains(&edge.source) && selected.contains(&edge.target))
            .take(max_results.saturating_mul(4))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        exported_edges
            .into_iter()
            .filter_map(|edge| {
                let source = edge.get("source").and_then(Value::as_str)?;
                let target = edge.get("target").and_then(Value::as_str)?;
                Some(GraphQueryEdge {
                    source: block_to_node.get(source)?.clone(),
                    target: block_to_node.get(target)?.clone(),
                    edge_type: edge
                        .get("relation")
                        .and_then(Value::as_str)
                        .unwrap_or("related")
                        .to_string(),
                })
            })
            .filter(|edge| selected.contains(&edge.source) && selected.contains(&edge.target))
            .take(max_results.saturating_mul(4))
            .collect::<Vec<_>>()
    };

    let python_stdout = response
        .get("stdout")
        .and_then(Value::as_str)
        .map(|stdout| truncate_python_stdout(stdout, PYTHON_QUERY_STDOUT_SUMMARY_LIMIT));
    let python_summary = response.get("summary").cloned();
    let python_usage = response.get("usage").cloned();
    let python_limits = response.get("limits").cloned();
    let python_result = response.get("result").cloned();
    let python_export_summary = export.and_then(|value| value.get("summary")).cloned();
    let visited_nodes = python_export_summary
        .as_ref()
        .and_then(|summary| summary.get("selected"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(nodes.len());
    let visited_edges = export
        .and_then(|value| value.get("total_selected_edges"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(edges.len());

    Ok(GraphQueryResult {
        query_kind: "python_query".to_string(),
        canonical_fingerprint: canonical_fingerprint.to_string(),
        max_results,
        truncated,
        duration_ms,
        cost: GraphQueryCost {
            visited_nodes,
            visited_edges,
        },
        nodes,
        edges,
        selected_block_ids,
        python_repo_name: Some(repo.repo_name.clone()),
        python_result,
        python_summary,
        python_usage,
        python_limits,
        python_stdout,
        python_export_summary,
    })
}

fn truncate_python_stdout(stdout: &str, limit: usize) -> String {
    let trimmed = stdout.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let retained = trimmed.chars().take(limit).collect::<String>();
    format!("{retained}…")
}

const PYTHON_GRAPH_QUERY_BOOTSTRAP: &str = r#"
import json
import sys
import traceback

def emit(payload):
    sys.stdout.write(json.dumps(payload))
    sys.stdout.flush()

try:
    import ucp
    payload = json.load(sys.stdin)
    graph = ucp.query(ucp.CodeGraph.from_json(payload['portable_document_json']))
    limits = payload.get('limits') or {}
    run = ucp.run_python_query(
        graph,
        payload['code'],
        bindings=payload.get('bindings') or {},
        include_export=payload.get('include_export', True),
        export_kwargs=payload.get('export_kwargs') or {},
        limits=ucp.QueryLimits(**limits),
    )
    emit(run.as_dict())
except Exception as exc:
    emit({
        'ok': False,
        'error': {
            'type': type(exc).__name__,
            'message': str(exc),
            'traceback': traceback.format_exc(),
        }
    })
"#;
