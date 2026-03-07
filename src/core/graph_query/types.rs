use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryError {
    pub code: String,
    pub message: String,
}
impl GraphQueryError {
    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryBounds {
    pub max_results_limit: usize,
    pub max_subgraph_depth: usize,
    pub default_max_results: usize,
}
impl Default for GraphQueryBounds {
    fn default() -> Self {
        Self {
            max_results_limit: MAX_RESULTS_LIMIT,
            max_subgraph_depth: MAX_SUBGRAPH_DEPTH,
            default_max_results: DEFAULT_MAX_RESULTS,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQueryRepository {
    pub repo_name: String,
    pub document: PortableDocument,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotFingerprintEntry {
    pub repo_name: String,
    pub repo_path: String,
    pub commit_hash: String,
    pub canonical_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GraphQueryRequest {
    Neighbors {
        node: String,
        #[serde(default)]
        edge_types: Vec<String>,
        #[serde(default)]
        max_results: Option<usize>,
    },
    Dependents {
        node: String,
        #[serde(default)]
        edge_types: Vec<String>,
        #[serde(default)]
        max_results: Option<usize>,
    },
    Subgraph {
        seed: String,
        depth: usize,
        #[serde(default)]
        edge_types: Vec<String>,
        #[serde(default)]
        max_results: Option<usize>,
    },
    Filter {
        #[serde(default)]
        node_type: Option<String>,
        #[serde(default)]
        path_prefix: Option<String>,
        #[serde(default)]
        partition: Option<String>,
        #[serde(default)]
        max_results: Option<usize>,
    },
}
impl GraphQueryRequest {
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Neighbors { .. } => "neighbors",
            Self::Dependents { .. } => "dependents",
            Self::Subgraph { .. } => "subgraph",
            Self::Filter { .. } => "filter",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GraphQueryNode {
    pub node_id: String,
    pub repo_name: String,
    pub logical_key: String,
    pub node_class: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub partition: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GraphQueryEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GraphQueryCost {
    pub visited_nodes: usize,
    pub visited_edges: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryResult {
    pub query_kind: String,
    pub canonical_fingerprint: String,
    pub max_results: usize,
    pub truncated: bool,
    #[serde(default)]
    pub duration_ms: u64,
    pub cost: GraphQueryCost,
    #[serde(default)]
    pub nodes: Vec<GraphQueryNode>,
    #[serde(default)]
    pub edges: Vec<GraphQueryEdge>,
}
