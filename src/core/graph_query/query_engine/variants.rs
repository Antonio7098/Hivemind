use super::super::support::{edge_filter_set, normalize_graph_path, truncate_node_ids};
use super::resolve::collect_nodes;
use super::*;
use std::collections::{BTreeSet, HashSet, VecDeque};

impl GraphQueryIndex {
    pub(crate) fn query_neighbors(
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
        let nodes = collect_nodes(&self.nodes, &node_ids);
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

    pub(crate) fn query_dependents(
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
        let nodes = collect_nodes(&self.nodes, &node_ids);
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
    pub(crate) fn query_subgraph(
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
        let nodes = collect_nodes(&self.nodes, &node_ids);
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

    pub(crate) fn query_filter(
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
        let nodes = collect_nodes(&self.nodes, &selected_ids);
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
}
