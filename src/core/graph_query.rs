//! Deterministic graph-query substrate over UCP graph snapshot artifacts.
//!
//! Sprint 46 introduces bounded graph query primitives that can be reused by
//! CLI inspection commands and native runtime tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ucp_api::PortableDocument;

mod query_engine;
mod support;

pub use support::{compute_snapshot_fingerprint, load_partition_paths_from_constitution};
use support::{match_partition, normalize_graph_path};

const DEFAULT_MAX_RESULTS: usize = 200;
const MAX_RESULTS_LIMIT: usize = 500;
const MAX_SUBGRAPH_DEPTH: usize = 4;
pub const GRAPH_QUERY_ENV_SNAPSHOT_PATH: &str = "HIVEMIND_GRAPH_SNAPSHOT_PATH";
pub const GRAPH_QUERY_ENV_CONSTITUTION_PATH: &str = "HIVEMIND_PROJECT_CONSTITUTION_PATH";
pub const GRAPH_QUERY_ENV_GATE_ERROR: &str = "HIVEMIND_GRAPH_QUERY_GATE_ERROR";
pub const GRAPH_QUERY_REFRESH_HINT: &str = "Run: hivemind graph snapshot refresh <project>";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GraphQueryError {
    pub code: String,
    pub message: String,
}

impl GraphQueryError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
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

#[derive(Debug, Clone)]
struct IndexedNode {
    repo_name: String,
    logical_key: String,
    node_class: String,
    path: Option<String>,
    partition: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GraphQueryIndex {
    nodes: BTreeMap<String, IndexedNode>,
    outgoing: BTreeMap<String, Vec<GraphQueryEdge>>,
    incoming: BTreeMap<String, Vec<GraphQueryEdge>>,
    all_edges: Vec<GraphQueryEdge>,
}

impl GraphQueryIndex {
    #[allow(clippy::too_many_lines)]
    pub fn from_snapshot_repositories(
        repositories: &[GraphQueryRepository],
        partition_paths: &BTreeMap<String, String>,
    ) -> Result<Self, GraphQueryError> {
        let mut nodes: BTreeMap<String, IndexedNode> = BTreeMap::new();
        let mut outgoing: BTreeMap<String, Vec<GraphQueryEdge>> = BTreeMap::new();
        let mut incoming: BTreeMap<String, Vec<GraphQueryEdge>> = BTreeMap::new();
        let mut all_edges = Vec::new();

        let mut block_to_node: HashMap<String, String> = HashMap::new();

        for repo in repositories {
            for (block_id, block) in &repo.document.blocks {
                let logical_key = block
                    .metadata
                    .custom
                    .get("logical_key")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if logical_key.is_empty() {
                    continue;
                }
                let node_class = block
                    .metadata
                    .custom
                    .get("node_class")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown")
                    .trim()
                    .to_string();
                let normalized_path = block
                    .metadata
                    .custom
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(normalize_graph_path)
                    .filter(|item| !item.is_empty());
                let partition = normalized_path
                    .as_deref()
                    .and_then(|path| match_partition(path, partition_paths));

                let node_id = format!("{}::{logical_key}", repo.repo_name);
                nodes.insert(
                    node_id.clone(),
                    IndexedNode {
                        repo_name: repo.repo_name.clone(),
                        logical_key,
                        node_class,
                        path: normalized_path,
                        partition,
                    },
                );
                block_to_node.insert(block_id.clone(), node_id);
            }
        }

        for repo in repositories {
            for (block_id, block) in &repo.document.blocks {
                let Some(source) = block_to_node.get(block_id).cloned() else {
                    continue;
                };
                let mut edges_for_source = Vec::new();
                for edge in &block.edges {
                    let target_key = edge.target.to_string();
                    let Some(target) = block_to_node.get(&target_key).cloned() else {
                        continue;
                    };
                    let edge_record = GraphQueryEdge {
                        source: source.clone(),
                        target,
                        edge_type: edge.edge_type.as_str().clone(),
                    };
                    edges_for_source.push(edge_record);
                }

                edges_for_source.sort_by(|a, b| {
                    a.edge_type
                        .cmp(&b.edge_type)
                        .then(a.target.cmp(&b.target))
                        .then(a.source.cmp(&b.source))
                });
                edges_for_source.dedup();
                if edges_for_source.is_empty() {
                    continue;
                }

                for edge in &edges_for_source {
                    incoming
                        .entry(edge.target.clone())
                        .or_default()
                        .push(edge.clone());
                    all_edges.push(edge.clone());
                }
                outgoing.entry(source).or_default().extend(edges_for_source);
            }
        }

        for edges in outgoing.values_mut() {
            edges.sort_by(|a, b| {
                a.edge_type
                    .cmp(&b.edge_type)
                    .then(a.target.cmp(&b.target))
                    .then(a.source.cmp(&b.source))
            });
            edges.dedup();
        }
        for edges in incoming.values_mut() {
            edges.sort_by(|a, b| {
                a.edge_type
                    .cmp(&b.edge_type)
                    .then(a.source.cmp(&b.source))
                    .then(a.target.cmp(&b.target))
            });
            edges.dedup();
        }
        all_edges.sort_by(|a, b| {
            a.edge_type
                .cmp(&b.edge_type)
                .then(a.source.cmp(&b.source))
                .then(a.target.cmp(&b.target))
        });
        all_edges.dedup();

        Ok(Self {
            nodes,
            outgoing,
            incoming,
            all_edges,
        })
    }
}
