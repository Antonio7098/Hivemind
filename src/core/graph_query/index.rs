use super::*;

#[derive(Debug, Clone)]
pub(crate) struct IndexedNode {
    pub(crate) repo_name: String,
    pub(crate) logical_key: String,
    pub(crate) node_class: String,
    pub(crate) path: Option<String>,
    pub(crate) partition: Option<String>,
}
#[derive(Debug, Clone)]
pub struct GraphQueryIndex {
    pub(crate) nodes: BTreeMap<String, IndexedNode>,
    pub(crate) outgoing: BTreeMap<String, Vec<GraphQueryEdge>>,
    pub(crate) incoming: BTreeMap<String, Vec<GraphQueryEdge>>,
    pub(crate) all_edges: Vec<GraphQueryEdge>,
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
