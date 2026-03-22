use super::*;
use ucp_codegraph::CodeGraphNavigator;

#[derive(Debug, Clone)]
pub(crate) struct IndexedNode {
    pub(crate) repo_name: String,
    pub(crate) logical_key: String,
    pub(crate) node_class: String,
    pub(crate) path: Option<String>,
    pub(crate) partition: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct IndexedRepository {
    pub(crate) navigator: CodeGraphNavigator,
    pub(crate) block_to_node_id: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct GraphQueryIndex {
    pub(crate) nodes: BTreeMap<String, IndexedNode>,
    pub(crate) outgoing: BTreeMap<String, Vec<GraphQueryEdge>>,
    pub(crate) incoming: BTreeMap<String, Vec<GraphQueryEdge>>,
    pub(crate) all_edges: Vec<GraphQueryEdge>,
    pub(crate) repositories: Vec<IndexedRepository>,
    pub(crate) node_to_repository_index: HashMap<String, usize>,
    pub(crate) node_to_block_selector: HashMap<String, String>,
}

fn block_graph_path(block: &ucm_core::Block) -> Option<String> {
    let metadata_path = block
        .metadata
        .custom
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(normalize_graph_path)
        .filter(|item| !item.is_empty());

    let metadata_coderef_path = block
        .metadata
        .custom
        .get("coderef")
        .and_then(|value| value.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(normalize_graph_path)
        .filter(|item| !item.is_empty());

    let content_coderef_path = match &block.content {
        ucm_core::Content::Json { value, .. } => value
            .get("coderef")
            .and_then(|value| value.get("path"))
            .and_then(serde_json::Value::as_str)
            .map(normalize_graph_path)
            .filter(|item| !item.is_empty()),
        _ => None,
    };

    let metadata_coderef_display = block
        .metadata
        .custom
        .get("coderef")
        .and_then(|value| value.get("display"))
        .and_then(serde_json::Value::as_str)
        .map(normalize_graph_path)
        .filter(|item| !item.is_empty());

    let content_coderef_display = match &block.content {
        ucm_core::Content::Json { value, .. } => value
            .get("coderef")
            .and_then(|value| value.get("display"))
            .and_then(serde_json::Value::as_str)
            .map(normalize_graph_path)
            .filter(|item| !item.is_empty()),
        _ => None,
    };

    metadata_path
        .or(metadata_coderef_path)
        .or(content_coderef_path)
        .or(metadata_coderef_display)
        .or(content_coderef_display)
}

impl GraphQueryIndex {
    // ARCH_DEBT: legacy oversized function
    #[allow(clippy::too_many_lines)]
    pub fn from_snapshot_repositories(
        repositories: &[GraphQueryRepository],
        partition_paths: &BTreeMap<String, String>,
    ) -> Result<Self, GraphQueryError> {
        let mut nodes: BTreeMap<String, IndexedNode> = BTreeMap::new();
        let mut outgoing: BTreeMap<String, Vec<GraphQueryEdge>> = BTreeMap::new();
        let mut incoming: BTreeMap<String, Vec<GraphQueryEdge>> = BTreeMap::new();
        let mut all_edges = Vec::new();
        let mut indexed_repositories = Vec::new();
        let mut node_to_repository_index = HashMap::new();
        let mut node_to_block_selector = HashMap::new();

        for repo in repositories {
            let document = repo.document.to_document().map_err(|error| {
                GraphQueryError::new(
                    "graph_query_snapshot_invalid",
                    format!(
                        "Repository '{}' contains an invalid portable graph document: {error}",
                        repo.repo_name
                    ),
                )
            })?;
            let navigator = CodeGraphNavigator::new(document.clone());
            let repository_index = indexed_repositories.len();
            let mut block_to_node: HashMap<String, String> = HashMap::new();

            for block_id in document.blocks.keys() {
                let Some(summary) = navigator.describe_node(*block_id) else {
                    continue;
                };
                let Some(block) = document.blocks.get(block_id) else {
                    continue;
                };
                let logical_key = summary.logical_key.unwrap_or_default().trim().to_string();
                if logical_key.is_empty() {
                    continue;
                }
                let node_class = if summary.node_class.trim().is_empty() {
                    block
                        .metadata
                        .custom
                        .get("node_class")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("unknown")
                        .trim()
                        .to_string()
                } else {
                    summary.node_class.trim().to_string()
                };
                let normalized_path = summary
                    .path
                    .map(|path| normalize_graph_path(&path))
                    .filter(|item| !item.is_empty())
                    .or_else(|| block_graph_path(block));
                let partition = normalized_path
                    .as_deref()
                    .and_then(|path| match_partition(path, partition_paths));

                let node_id = format!("{}::{logical_key}", repo.repo_name);
                let block_selector = block_id.to_string();
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
                block_to_node.insert(block_selector.clone(), node_id.clone());
                node_to_repository_index.insert(node_id.clone(), repository_index);
                node_to_block_selector.insert(node_id, block_selector);
            }

            for (block_id, block) in &document.blocks {
                let block_key = block_id.to_string();
                let Some(source) = block_to_node.get(&block_key).cloned() else {
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

            indexed_repositories.push(IndexedRepository {
                navigator,
                block_to_node_id: block_to_node,
            });
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
            repositories: indexed_repositories,
            node_to_repository_index,
            node_to_block_selector,
        })
    }
}
