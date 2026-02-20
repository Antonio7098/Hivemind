//! Deterministic graph-query substrate over UCP graph snapshot artifacts.
//!
//! Sprint 46 introduces bounded graph query primitives that can be reused by
//! CLI inspection commands and native runtime tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;
use ucp_api::PortableDocument;

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

    pub fn execute(
        &self,
        request: &GraphQueryRequest,
        canonical_fingerprint: &str,
        bounds: &GraphQueryBounds,
    ) -> Result<GraphQueryResult, GraphQueryError> {
        let max_results = request_max_results(request, bounds)?;
        match request {
            GraphQueryRequest::Neighbors {
                node, edge_types, ..
            } => self.query_neighbors(
                node,
                edge_types,
                max_results,
                canonical_fingerprint,
                request.kind(),
            ),
            GraphQueryRequest::Dependents {
                node, edge_types, ..
            } => self.query_dependents(
                node,
                edge_types,
                max_results,
                canonical_fingerprint,
                request.kind(),
            ),
            GraphQueryRequest::Subgraph {
                seed,
                depth,
                edge_types,
                ..
            } => {
                if *depth == 0 {
                    return Err(GraphQueryError::new(
                        "graph_query_depth_invalid",
                        "Subgraph depth must be >= 1",
                    ));
                }
                if *depth > bounds.max_subgraph_depth {
                    return Err(GraphQueryError::new(
                        "graph_query_depth_exceeded",
                        format!(
                            "Subgraph depth {} exceeds limit {}",
                            depth, bounds.max_subgraph_depth
                        ),
                    ));
                }
                self.query_subgraph(
                    seed,
                    *depth,
                    edge_types,
                    max_results,
                    canonical_fingerprint,
                    request.kind(),
                )
            }
            GraphQueryRequest::Filter {
                node_type,
                path_prefix,
                partition,
                ..
            } => Ok(self.query_filter(
                node_type.as_deref(),
                path_prefix.as_deref(),
                partition.as_deref(),
                max_results,
                canonical_fingerprint,
                request.kind(),
            )),
        }
    }

    fn query_neighbors(
        &self,
        node_ref: &str,
        edge_types: &[String],
        max_results: usize,
        canonical_fingerprint: &str,
        query_kind: &str,
    ) -> Result<GraphQueryResult, GraphQueryError> {
        let source = self.resolve_node_id(node_ref)?;
        let edge_filter = edge_filter_set(edge_types);
        let outgoing = self
            .outgoing
            .get(&source)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|edge| {
                edge_filter
                    .as_ref()
                    .is_none_or(|set| set.contains(&edge.edge_type))
            })
            .collect::<Vec<_>>();

        let visited_edges = outgoing.len();
        let mut node_ids: BTreeSet<String> = BTreeSet::new();
        node_ids.insert(source);
        for edge in &outgoing {
            node_ids.insert(edge.target.clone());
        }

        let (node_ids, truncated) = truncate_node_ids(node_ids, max_results);
        let nodes = self.collect_nodes(&node_ids);
        let edges = outgoing
            .into_iter()
            .filter(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target))
            .collect::<Vec<_>>();

        Ok(GraphQueryResult {
            query_kind: query_kind.to_string(),
            canonical_fingerprint: canonical_fingerprint.to_string(),
            max_results,
            truncated,
            duration_ms: 0,
            cost: GraphQueryCost {
                visited_nodes: nodes.len(),
                visited_edges,
            },
            nodes,
            edges,
        })
    }

    fn query_dependents(
        &self,
        node_ref: &str,
        edge_types: &[String],
        max_results: usize,
        canonical_fingerprint: &str,
        query_kind: &str,
    ) -> Result<GraphQueryResult, GraphQueryError> {
        let target = self.resolve_node_id(node_ref)?;
        let edge_filter = edge_filter_set(edge_types);
        let incoming = self
            .incoming
            .get(&target)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|edge| {
                edge_filter
                    .as_ref()
                    .is_none_or(|set| set.contains(&edge.edge_type))
            })
            .collect::<Vec<_>>();

        let visited_edges = incoming.len();
        let mut node_ids: BTreeSet<String> = BTreeSet::new();
        node_ids.insert(target);
        for edge in &incoming {
            node_ids.insert(edge.source.clone());
        }

        let (node_ids, truncated) = truncate_node_ids(node_ids, max_results);
        let nodes = self.collect_nodes(&node_ids);
        let edges = incoming
            .into_iter()
            .filter(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target))
            .collect::<Vec<_>>();

        Ok(GraphQueryResult {
            query_kind: query_kind.to_string(),
            canonical_fingerprint: canonical_fingerprint.to_string(),
            max_results,
            truncated,
            duration_ms: 0,
            cost: GraphQueryCost {
                visited_nodes: nodes.len(),
                visited_edges,
            },
            nodes,
            edges,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn query_subgraph(
        &self,
        seed_ref: &str,
        depth: usize,
        edge_types: &[String],
        max_results: usize,
        canonical_fingerprint: &str,
        query_kind: &str,
    ) -> Result<GraphQueryResult, GraphQueryError> {
        let seed = self.resolve_node_id(seed_ref)?;
        let edge_filter = edge_filter_set(edge_types);

        let mut visited_node_ids: BTreeSet<String> = BTreeSet::new();
        let mut emitted_edges: BTreeSet<(String, String, String)> = BTreeSet::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        let mut scheduled: HashSet<String> = HashSet::new();
        let mut visited_edges = 0_usize;

        queue.push_back((seed.clone(), 0));
        scheduled.insert(seed);

        while let Some((node, distance)) = queue.pop_front() {
            visited_node_ids.insert(node.clone());
            if distance >= depth {
                continue;
            }

            let edges = self.outgoing.get(&node).cloned().unwrap_or_default();
            for edge in edges {
                visited_edges = visited_edges.saturating_add(1);
                if edge_filter
                    .as_ref()
                    .is_some_and(|set| !set.contains(&edge.edge_type))
                {
                    continue;
                }
                emitted_edges.insert((
                    edge.source.clone(),
                    edge.target.clone(),
                    edge.edge_type.clone(),
                ));

                if scheduled.insert(edge.target.clone()) {
                    queue.push_back((edge.target, distance.saturating_add(1)));
                }
            }
        }

        let (node_ids, truncated) = truncate_node_ids(visited_node_ids, max_results);
        let nodes = self.collect_nodes(&node_ids);
        let edges = emitted_edges
            .into_iter()
            .map(|(source, target, edge_type)| GraphQueryEdge {
                source,
                target,
                edge_type,
            })
            .filter(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target))
            .collect::<Vec<_>>();

        Ok(GraphQueryResult {
            query_kind: query_kind.to_string(),
            canonical_fingerprint: canonical_fingerprint.to_string(),
            max_results,
            truncated,
            duration_ms: 0,
            cost: GraphQueryCost {
                visited_nodes: nodes.len(),
                visited_edges,
            },
            nodes,
            edges,
        })
    }

    fn query_filter(
        &self,
        node_type: Option<&str>,
        path_prefix: Option<&str>,
        partition: Option<&str>,
        max_results: usize,
        canonical_fingerprint: &str,
        query_kind: &str,
    ) -> GraphQueryResult {
        let node_type = node_type.map(str::trim).filter(|value| !value.is_empty());
        let path_prefix = path_prefix
            .map(normalize_graph_path)
            .filter(|value| !value.is_empty());
        let partition = partition.map(str::trim).filter(|value| !value.is_empty());

        let mut selected_ids = BTreeSet::new();
        for (node_id, node) in &self.nodes {
            if node_type.is_some_and(|expected| node.node_class != expected) {
                continue;
            }
            if partition.is_some_and(|expected| node.partition.as_deref() != Some(expected)) {
                continue;
            }
            if path_prefix.as_ref().is_some_and(|prefix| {
                node.path
                    .as_deref()
                    .is_none_or(|path| !(path == prefix || path.starts_with(&format!("{prefix}/"))))
            }) {
                continue;
            }
            selected_ids.insert(node_id.clone());
        }

        let visited_nodes = self.nodes.len();
        let visited_edges = self.all_edges.len();

        let (selected_ids, truncated) = truncate_node_ids(selected_ids, max_results);
        let nodes = self.collect_nodes(&selected_ids);
        let edges = self
            .all_edges
            .iter()
            .filter(|edge| {
                selected_ids.contains(&edge.source) && selected_ids.contains(&edge.target)
            })
            .cloned()
            .collect::<Vec<_>>();

        GraphQueryResult {
            query_kind: query_kind.to_string(),
            canonical_fingerprint: canonical_fingerprint.to_string(),
            max_results,
            truncated,
            duration_ms: 0,
            cost: GraphQueryCost {
                visited_nodes,
                visited_edges,
            },
            nodes,
            edges,
        }
    }

    fn collect_nodes(&self, node_ids: &BTreeSet<String>) -> Vec<GraphQueryNode> {
        node_ids
            .iter()
            .filter_map(|node_id| {
                self.nodes.get(node_id).map(|node| GraphQueryNode {
                    node_id: node_id.clone(),
                    repo_name: node.repo_name.clone(),
                    logical_key: node.logical_key.clone(),
                    node_class: node.node_class.clone(),
                    path: node.path.clone(),
                    partition: node.partition.clone(),
                })
            })
            .collect()
    }

    fn resolve_node_id(&self, input: &str) -> Result<String, GraphQueryError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(GraphQueryError::new(
                "graph_query_node_invalid",
                "Node reference cannot be empty",
            ));
        }
        if self.nodes.contains_key(trimmed) {
            return Ok(trimmed.to_string());
        }

        let normalized = normalize_graph_path(trimmed);
        let mut matches = Vec::new();
        for (node_id, node) in &self.nodes {
            let logical_match = node.logical_key == trimmed;
            let path_match = node.path.as_deref() == Some(normalized.as_str());
            if logical_match || path_match {
                matches.push(node_id.clone());
            }
        }

        matches.sort();
        matches.dedup();
        if matches.len() == 1 {
            return Ok(matches[0].clone());
        }
        if matches.is_empty() {
            return Err(GraphQueryError::new(
                "graph_query_node_not_found",
                format!("Node '{trimmed}' was not found in graph snapshot"),
            ));
        }
        Err(GraphQueryError::new(
            "graph_query_node_ambiguous",
            format!(
                "Node reference '{trimmed}' is ambiguous ({} matches)",
                matches.len()
            ),
        ))
    }
}

#[must_use]
pub fn load_partition_paths_from_constitution(path: &Path) -> BTreeMap<String, String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw) else {
        return BTreeMap::new();
    };
    let Some(partitions) = value
        .get("partitions")
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for item in partitions {
        let Some(id) = item
            .get("id")
            .and_then(serde_yaml::Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        let Some(path_value) = item
            .get("path")
            .and_then(serde_yaml::Value::as_str)
            .map(normalize_graph_path)
            .filter(|v| !v.is_empty())
        else {
            continue;
        };
        out.insert(id.to_string(), path_value);
    }
    out
}

#[must_use]
pub fn compute_snapshot_fingerprint(entries: &[SnapshotFingerprintEntry]) -> String {
    let mut ordered = entries
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}",
                entry.repo_name, entry.repo_path, entry.commit_hash, entry.canonical_fingerprint
            )
        })
        .collect::<Vec<_>>();
    ordered.sort();
    digest_hex(ordered.join("\n").as_bytes())
}

fn request_max_results(
    request: &GraphQueryRequest,
    bounds: &GraphQueryBounds,
) -> Result<usize, GraphQueryError> {
    let raw = match request {
        GraphQueryRequest::Neighbors { max_results, .. }
        | GraphQueryRequest::Dependents { max_results, .. }
        | GraphQueryRequest::Subgraph { max_results, .. }
        | GraphQueryRequest::Filter { max_results, .. } => *max_results,
    }
    .unwrap_or(bounds.default_max_results);

    if raw == 0 {
        return Err(GraphQueryError::new(
            "graph_query_bounds_invalid",
            "max_results must be > 0",
        ));
    }
    if raw > bounds.max_results_limit {
        return Err(GraphQueryError::new(
            "graph_query_bounds_exceeded",
            format!(
                "max_results {} exceeds limit {}",
                raw, bounds.max_results_limit
            ),
        ));
    }
    Ok(raw)
}

fn truncate_node_ids(node_ids: BTreeSet<String>, max_results: usize) -> (BTreeSet<String>, bool) {
    if node_ids.len() <= max_results {
        return (node_ids, false);
    }
    let retained = node_ids.into_iter().take(max_results).collect();
    (retained, true)
}

fn edge_filter_set(edge_types: &[String]) -> Option<BTreeSet<String>> {
    let mut normalized = edge_types
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if normalized.is_empty() {
        return None;
    }
    if normalized.remove("*") {
        return None;
    }
    Some(normalized)
}

fn normalize_graph_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .to_string()
}

fn match_partition(path: &str, partitions: &BTreeMap<String, String>) -> Option<String> {
    let mut candidates = partitions
        .iter()
        .filter_map(|(partition_id, partition_path)| {
            if path == partition_path || path.starts_with(&format!("{partition_path}/")) {
                Some((partition_path.len(), partition_id.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    candidates.first().map(|(_, id)| id.clone())
}

fn digest_hex(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[allow(clippy::too_many_lines)]
    fn sample_index() -> GraphQueryIndex {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "repo::a".to_string(),
            IndexedNode {
                repo_name: "repo".to_string(),
                logical_key: "a".to_string(),
                node_class: "file".to_string(),
                path: Some("src/a.rs".to_string()),
                partition: Some("core".to_string()),
            },
        );
        nodes.insert(
            "repo::b".to_string(),
            IndexedNode {
                repo_name: "repo".to_string(),
                logical_key: "b".to_string(),
                node_class: "file".to_string(),
                path: Some("src/b.rs".to_string()),
                partition: Some("core".to_string()),
            },
        );
        nodes.insert(
            "repo::c".to_string(),
            IndexedNode {
                repo_name: "repo".to_string(),
                logical_key: "c".to_string(),
                node_class: "file".to_string(),
                path: Some("src/c.rs".to_string()),
                partition: Some("api".to_string()),
            },
        );
        nodes.insert(
            "repo::d".to_string(),
            IndexedNode {
                repo_name: "repo".to_string(),
                logical_key: "d".to_string(),
                node_class: "symbol".to_string(),
                path: Some("src/d.rs".to_string()),
                partition: Some("api".to_string()),
            },
        );

        let mut outgoing = BTreeMap::new();
        outgoing.insert(
            "repo::a".to_string(),
            vec![
                GraphQueryEdge {
                    source: "repo::a".to_string(),
                    target: "repo::c".to_string(),
                    edge_type: "calls".to_string(),
                },
                GraphQueryEdge {
                    source: "repo::a".to_string(),
                    target: "repo::b".to_string(),
                    edge_type: "imports".to_string(),
                },
                GraphQueryEdge {
                    source: "repo::a".to_string(),
                    target: "repo::d".to_string(),
                    edge_type: "uses".to_string(),
                },
            ],
        );
        outgoing.insert(
            "repo::b".to_string(),
            vec![GraphQueryEdge {
                source: "repo::b".to_string(),
                target: "repo::c".to_string(),
                edge_type: "imports".to_string(),
            }],
        );

        let mut incoming = BTreeMap::new();
        incoming.insert(
            "repo::b".to_string(),
            vec![GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::b".to_string(),
                edge_type: "imports".to_string(),
            }],
        );
        incoming.insert(
            "repo::c".to_string(),
            vec![
                GraphQueryEdge {
                    source: "repo::a".to_string(),
                    target: "repo::c".to_string(),
                    edge_type: "calls".to_string(),
                },
                GraphQueryEdge {
                    source: "repo::b".to_string(),
                    target: "repo::c".to_string(),
                    edge_type: "imports".to_string(),
                },
            ],
        );
        incoming.insert(
            "repo::d".to_string(),
            vec![GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::d".to_string(),
                edge_type: "uses".to_string(),
            }],
        );

        let all_edges = vec![
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::c".to_string(),
                edge_type: "calls".to_string(),
            },
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::b".to_string(),
                edge_type: "imports".to_string(),
            },
            GraphQueryEdge {
                source: "repo::b".to_string(),
                target: "repo::c".to_string(),
                edge_type: "imports".to_string(),
            },
            GraphQueryEdge {
                source: "repo::a".to_string(),
                target: "repo::d".to_string(),
                edge_type: "uses".to_string(),
            },
        ];

        GraphQueryIndex {
            nodes,
            outgoing,
            incoming,
            all_edges,
        }
    }

    #[test]
    fn neighbors_is_deterministic_and_bounded() {
        let index = sample_index();
        let request = GraphQueryRequest::Neighbors {
            node: "repo::a".to_string(),
            edge_types: Vec::new(),
            max_results: Some(3),
        };
        let result = index
            .execute(&request, "fp", &GraphQueryBounds::default())
            .expect("query should execute");

        let ids = result
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["repo::a", "repo::b", "repo::c"]);
        assert!(result.truncated);
        assert_eq!(result.cost.visited_edges, 3);
        assert_eq!(result.edges.len(), 2);
    }

    #[test]
    fn subgraph_depth_respects_upper_bound() {
        let index = sample_index();
        let request = GraphQueryRequest::Subgraph {
            seed: "repo::a".to_string(),
            depth: 5,
            edge_types: Vec::new(),
            max_results: Some(20),
        };
        let error = index
            .execute(&request, "fp", &GraphQueryBounds::default())
            .expect_err("depth over limit should fail");
        assert_eq!(error.code, "graph_query_depth_exceeded");
    }

    #[test]
    fn filter_matches_partition_and_path_prefix() {
        let index = sample_index();
        let request = GraphQueryRequest::Filter {
            node_type: Some("file".to_string()),
            path_prefix: Some("src".to_string()),
            partition: Some("core".to_string()),
            max_results: Some(10),
        };
        let result = index
            .execute(&request, "fp", &GraphQueryBounds::default())
            .expect("query should execute");

        let ids = result
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["repo::a", "repo::b"]);
        assert!(!result.truncated);
    }

    #[test]
    fn load_partition_paths_ignores_invalid_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let constitution_path = tmp.path().join("constitution.yaml");
        fs::write(
            &constitution_path,
            r#"
partitions:
  - id: core
    path: src/core
  - id: empty
    path: ""
  - id: missing
"#,
        )
        .expect("write constitution");

        let partitions = load_partition_paths_from_constitution(&constitution_path);
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions.get("core"), Some(&"src/core".to_string()));
    }

    #[test]
    fn snapshot_fingerprint_is_order_invariant() {
        let entries = vec![
            SnapshotFingerprintEntry {
                repo_name: "a".to_string(),
                repo_path: "/tmp/a".to_string(),
                commit_hash: "aaa".to_string(),
                canonical_fingerprint: "111".to_string(),
            },
            SnapshotFingerprintEntry {
                repo_name: "b".to_string(),
                repo_path: "/tmp/b".to_string(),
                commit_hash: "bbb".to_string(),
                canonical_fingerprint: "222".to_string(),
            },
        ];
        let mut reversed = entries.clone();
        reversed.reverse();
        assert_eq!(
            compute_snapshot_fingerprint(&entries),
            compute_snapshot_fingerprint(&reversed)
        );
        assert_ne!(
            compute_snapshot_fingerprint(&entries),
            compute_snapshot_fingerprint(&[
                SnapshotFingerprintEntry {
                    repo_name: "a".to_string(),
                    repo_path: PathBuf::from("/tmp/a").display().to_string(),
                    commit_hash: "changed".to_string(),
                    canonical_fingerprint: "111".to_string(),
                },
                SnapshotFingerprintEntry {
                    repo_name: "b".to_string(),
                    repo_path: "/tmp/b".to_string(),
                    commit_hash: "bbb".to_string(),
                    canonical_fingerprint: "222".to_string(),
                }
            ])
        );
    }
}
