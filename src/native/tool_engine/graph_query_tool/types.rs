use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum GraphQueryKindInput {
    Neighbors,
    Dependents,
    Subgraph,
    Filter,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
}
impl GraphQueryInput {
    pub(super) fn into_request(self) -> Result<GraphQueryRequest, NativeToolEngineError> {
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
