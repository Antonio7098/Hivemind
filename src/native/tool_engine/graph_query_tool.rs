use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum GraphQueryKindInput {
    Neighbors,
    Dependents,
    Subgraph,
    Filter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphQueryInput {
    kind: GraphQueryKindInput,
    #[serde(default)]
    node: Option<String>,
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    edge_types: Vec<String>,
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    path_prefix: Option<String>,
    #[serde(default)]
    partition: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

impl GraphQueryInput {
    fn into_request(self) -> Result<GraphQueryRequest, NativeToolEngineError> {
        match self.kind {
            GraphQueryKindInput::Neighbors => Ok(GraphQueryRequest::Neighbors {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.neighbors requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Dependents => Ok(GraphQueryRequest::Dependents {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.dependents requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Subgraph => Ok(GraphQueryRequest::Subgraph {
                seed: self.seed.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'seed'")
                })?,
                depth: self.depth.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'depth'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Filter => Ok(GraphQueryRequest::Filter {
                node_type: self.node_type,
                path_prefix: self.path_prefix,
                partition: self.partition,
                max_results: self.max_results,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphQueryGateError {
    code: String,
    message: String,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeGraphSnapshotCommit {
    pub(super) repo_name: String,
    pub(super) repo_path: String,
    pub(super) commit_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeGraphSnapshotProvenance {
    pub(super) head_commits: Vec<RuntimeGraphSnapshotCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeGraphSnapshotRepository {
    pub(super) repo_name: String,
    pub(super) repo_path: String,
    pub(super) commit_hash: String,
    pub(super) canonical_fingerprint: String,
    pub(super) document: PortableDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeGraphSnapshotArtifact {
    pub(super) profile_version: String,
    pub(super) canonical_fingerprint: String,
    pub(super) provenance: RuntimeGraphSnapshotProvenance,
    pub(super) repositories: Vec<RuntimeGraphSnapshotRepository>,
}

fn graph_query_runtime_error(code: &str, message: impl Into<String>) -> NativeToolEngineError {
    NativeToolEngineError::new(code, message, false)
}

fn graph_query_runtime_error_with_refresh_hint(
    code: &str,
    message: impl Into<String>,
) -> NativeToolEngineError {
    graph_query_runtime_error(
        code,
        format!("{}. Hint: {GRAPH_QUERY_REFRESH_HINT}", message.into()),
    )
}

fn resolve_repo_head_commit(
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

fn graph_query_bounds_from_env(
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

fn load_runtime_graph_snapshot(
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

fn validate_runtime_graph_snapshot(
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

pub(super) fn aggregate_snapshot_fingerprint_registry_style(
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

fn map_graph_query_error(
    error: crate::core::graph_query::GraphQueryError,
) -> NativeToolEngineError {
    NativeToolEngineError::new(error.code, error.message, false)
}

pub(super) fn handle_graph_query(
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
