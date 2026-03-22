use super::*;
use crate::core::graph_query::GraphQueryNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum GraphQueryKindInput {
    Neighbors,
    Dependents,
    Subgraph,
    Filter,
    PythonQuery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphQueryPythonLimitsInput {
    #[serde(default)]
    pub(super) max_seconds: Option<f64>,
    #[serde(default)]
    pub(super) max_operations: Option<usize>,
    #[serde(default)]
    pub(super) max_trace_events: Option<usize>,
    #[serde(default)]
    pub(super) max_stdout_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphQueryPythonInput {
    pub(super) repo_name: Option<String>,
    pub(super) code: String,
    #[serde(default)]
    pub(super) bindings: BTreeMap<String, Value>,
    #[serde(default)]
    pub(super) include_export: Option<bool>,
    #[serde(default)]
    pub(super) export_kwargs: BTreeMap<String, Value>,
    #[serde(default)]
    pub(super) limits: Option<GraphQueryPythonLimitsInput>,
    #[serde(default)]
    pub(super) max_results: Option<usize>,
}

#[derive(Debug, Clone)]
pub(super) enum GraphQueryToolRequest {
    Native(GraphQueryRequest),
    Python(GraphQueryPythonInput),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GraphQueryInput {
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
    #[serde(default)]
    repo_name: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    bindings: BTreeMap<String, Value>,
    #[serde(default)]
    include_export: Option<bool>,
    #[serde(default)]
    export_kwargs: BTreeMap<String, Value>,
    #[serde(default)]
    limits: Option<GraphQueryPythonLimitsInput>,
}
impl GraphQueryInput {
    pub(super) fn into_request(self) -> Result<GraphQueryToolRequest, NativeToolEngineError> {
        match self.kind {
            GraphQueryKindInput::Neighbors => Ok(GraphQueryToolRequest::Native(
                GraphQueryRequest::Neighbors {
                    node: self.node.ok_or_else(|| {
                        NativeToolEngineError::validation("graph_query.neighbors requires 'node'")
                    })?,
                    edge_types: self.edge_types,
                    max_results: self.max_results,
                },
            )),
            GraphQueryKindInput::Dependents => Ok(GraphQueryToolRequest::Native(
                GraphQueryRequest::Dependents {
                    node: self.node.ok_or_else(|| {
                        NativeToolEngineError::validation("graph_query.dependents requires 'node'")
                    })?,
                    edge_types: self.edge_types,
                    max_results: self.max_results,
                },
            )),
            GraphQueryKindInput::Subgraph => {
                Ok(GraphQueryToolRequest::Native(GraphQueryRequest::Subgraph {
                    seed: self.seed.ok_or_else(|| {
                        NativeToolEngineError::validation("graph_query.subgraph requires 'seed'")
                    })?,
                    depth: self.depth.ok_or_else(|| {
                        NativeToolEngineError::validation("graph_query.subgraph requires 'depth'")
                    })?,
                    edge_types: self.edge_types,
                    max_results: self.max_results,
                }))
            }
            GraphQueryKindInput::Filter => {
                Ok(GraphQueryToolRequest::Native(GraphQueryRequest::Filter {
                    node_type: self.node_type,
                    path_prefix: self.path_prefix,
                    partition: self.partition,
                    max_results: self.max_results,
                }))
            }
            GraphQueryKindInput::PythonQuery => {
                Ok(GraphQueryToolRequest::Python(GraphQueryPythonInput {
                    repo_name: self.repo_name,
                    code: self.code.ok_or_else(|| {
                        NativeToolEngineError::validation(
                            "graph_query.python_query requires 'code'",
                        )
                    })?,
                    bindings: self.bindings,
                    include_export: self.include_export,
                    export_kwargs: self.export_kwargs,
                    limits: self.limits,
                    max_results: self.max_results,
                }))
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct RuntimeGraphQueryGateError {
    pub(super) code: String,
    pub(super) message: String,
    #[serde(default)]
    pub(super) hint: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphSnapshotCommit {
    pub(crate) repo_name: String,
    pub(crate) repo_path: String,
    pub(crate) commit_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphSnapshotProvenance {
    pub(crate) head_commits: Vec<RuntimeGraphSnapshotCommit>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphSnapshotRepository {
    pub(crate) repo_name: String,
    pub(crate) repo_path: String,
    pub(crate) commit_hash: String,
    pub(crate) canonical_fingerprint: String,
    pub(crate) document: PortableDocument,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeGraphSnapshotArtifact {
    pub(crate) profile_version: String,
    pub(crate) canonical_fingerprint: String,
    pub(crate) provenance: RuntimeGraphSnapshotProvenance,
    pub(crate) repositories: Vec<RuntimeGraphSnapshotRepository>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersistedGraphQueryNodeRef {
    pub(crate) node_id: String,
    #[serde(default)]
    pub(crate) repo_name: String,
    #[serde(default)]
    pub(crate) logical_key: String,
    #[serde(default)]
    pub(crate) node_class: String,
    #[serde(default)]
    pub(crate) path: Option<String>,
    #[serde(default)]
    pub(crate) partition: Option<String>,
}

impl PersistedGraphQueryNodeRef {
    pub(crate) fn from_graph_node(node: &GraphQueryNode) -> Self {
        Self {
            node_id: node.node_id.clone(),
            repo_name: node.repo_name.clone(),
            logical_key: node.logical_key.clone(),
            node_class: node.node_class.clone(),
            path: node.path.clone(),
            partition: node.partition.clone(),
        }
    }

    pub(crate) fn from_node_id(node_id: String) -> Self {
        let (repo_name, logical_key) = node_id.split_once("::").map_or_else(
            || (String::new(), String::new()),
            |(repo_name, logical_key)| (repo_name.to_string(), logical_key.to_string()),
        );
        Self {
            node_id,
            repo_name,
            logical_key,
            node_class: String::new(),
            path: None,
            partition: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PersistedGraphQueryTraversal {
    pub(crate) query_kind: String,
    pub(crate) max_results: usize,
    pub(crate) truncated: bool,
    pub(crate) duration_ms: u64,
    #[serde(default)]
    pub(crate) node_ids: Vec<String>,
    #[serde(default)]
    pub(crate) logical_keys: Vec<String>,
    #[serde(default)]
    pub(crate) paths: Vec<String>,
}
