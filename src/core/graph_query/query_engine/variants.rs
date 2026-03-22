use super::super::support::{edge_filter_set, normalize_graph_path, truncate_node_ids};
use super::resolve::collect_nodes;
use super::*;
use std::collections::{BTreeSet, HashSet, VecDeque};
use ucp_graph::GraphNeighborMode;

impl GraphQueryIndex {
    pub(crate) fn query_neighbors(
        &self,
        node_ref: &str,
        edge_types: &[String],
        max_results: usize,
        canonical_fingerprint: &str,
        query_kind: &str,
    ) -> Result<GraphQueryResult, GraphQueryError> {
        if !self.repositories.is_empty() {
            let source = self.resolve_runtime_node(node_ref)?;
            let repository = self
                .repositories
                .get(source.repository_index)
                .ok_or_else(|| {
                    GraphQueryError::new(
                        "graph_query_runtime_unavailable",
                        format!(
                            "Node '{}' resolved to an unavailable local GraphCode repository",
                            source.node_id
                        ),
                    )
                })?;
            let block_id = repository
                .navigator
                .resolve_selector(&source.block_selector)
                .ok_or_else(|| {
                    GraphQueryError::new(
                        "graph_query_node_not_found",
                        format!(
                            "Node '{}' was not found in the local GraphCode runtime",
                            source.node_id
                        ),
                    )
                })?;
            let edge_filter = edge_filter_set(edge_types);
            let mut outgoing = repository
                .navigator
                .graph()
                .neighbors(block_id, GraphNeighborMode::Outgoing)
                .into_iter()
                .filter(|edge| {
                    edge_filter
                        .as_ref()
                        .is_none_or(|set| set.contains(&edge.relation))
                })
                .filter_map(|edge| {
                    self.runtime_node_id_for_block(source.repository_index, &edge.to.to_string())
                        .map(|target| GraphQueryEdge {
                            source: source.node_id.clone(),
                            target,
                            edge_type: edge.relation,
                        })
                })
                .collect::<Vec<_>>();
            outgoing.sort_by(|a, b| {
                a.edge_type
                    .cmp(&b.edge_type)
                    .then(a.target.cmp(&b.target))
                    .then(a.source.cmp(&b.source))
            });
            outgoing.dedup();

            let visited_edges = outgoing.len();
            let mut node_ids: BTreeSet<String> = BTreeSet::new();
            node_ids.insert(source.node_id.clone());
            for edge in &outgoing {
                node_ids.insert(edge.target.clone());
            }

            let (node_ids, truncated) = truncate_node_ids(node_ids, max_results);
            let nodes = collect_nodes(&self.nodes, &node_ids);
            let edges = outgoing
                .into_iter()
                .filter(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target))
                .collect::<Vec<_>>();

            return Ok(GraphQueryResult {
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
                selected_block_ids: Vec::new(),
                python_repo_name: None,
                python_result: None,
                python_summary: None,
                python_usage: None,
                python_limits: None,
                python_stdout: None,
                python_export_summary: None,
            });
        }

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
            selected_block_ids: Vec::new(),
            python_repo_name: None,
            python_result: None,
            python_summary: None,
            python_usage: None,
            python_limits: None,
            python_stdout: None,
            python_export_summary: None,
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
        if !self.repositories.is_empty() {
            let target = self.resolve_runtime_node(node_ref)?;
            let repository = self
                .repositories
                .get(target.repository_index)
                .ok_or_else(|| {
                    GraphQueryError::new(
                        "graph_query_runtime_unavailable",
                        format!(
                            "Node '{}' resolved to an unavailable local GraphCode repository",
                            target.node_id
                        ),
                    )
                })?;
            let block_id = repository
                .navigator
                .resolve_selector(&target.block_selector)
                .ok_or_else(|| {
                    GraphQueryError::new(
                        "graph_query_node_not_found",
                        format!(
                            "Node '{}' was not found in the local GraphCode runtime",
                            target.node_id
                        ),
                    )
                })?;
            let edge_filter = edge_filter_set(edge_types);
            let mut incoming = repository
                .navigator
                .graph()
                .neighbors(block_id, GraphNeighborMode::Incoming)
                .into_iter()
                .filter(|edge| {
                    edge_filter
                        .as_ref()
                        .is_none_or(|set| set.contains(&edge.relation))
                })
                .filter_map(|edge| {
                    self.runtime_node_id_for_block(target.repository_index, &edge.to.to_string())
                        .map(|source| GraphQueryEdge {
                            source,
                            target: target.node_id.clone(),
                            edge_type: edge.relation,
                        })
                })
                .collect::<Vec<_>>();
            incoming.sort_by(|a, b| {
                a.edge_type
                    .cmp(&b.edge_type)
                    .then(a.source.cmp(&b.source))
                    .then(a.target.cmp(&b.target))
            });
            incoming.dedup();

            let visited_edges = incoming.len();
            let mut node_ids: BTreeSet<String> = BTreeSet::new();
            node_ids.insert(target.node_id.clone());
            for edge in &incoming {
                node_ids.insert(edge.source.clone());
            }

            let (node_ids, truncated) = truncate_node_ids(node_ids, max_results);
            let nodes = collect_nodes(&self.nodes, &node_ids);
            let edges = incoming
                .into_iter()
                .filter(|edge| node_ids.contains(&edge.source) && node_ids.contains(&edge.target))
                .collect::<Vec<_>>();

            return Ok(GraphQueryResult {
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
                selected_block_ids: Vec::new(),
                python_repo_name: None,
                python_result: None,
                python_summary: None,
                python_usage: None,
                python_limits: None,
                python_stdout: None,
                python_export_summary: None,
            });
        }

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
            selected_block_ids: Vec::new(),
            python_repo_name: None,
            python_result: None,
            python_summary: None,
            python_usage: None,
            python_limits: None,
            python_stdout: None,
            python_export_summary: None,
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
        if !self.repositories.is_empty() {
            let seed = self.resolve_runtime_node(seed_ref)?;
            let repository = self
                .repositories
                .get(seed.repository_index)
                .ok_or_else(|| {
                    GraphQueryError::new(
                        "graph_query_runtime_unavailable",
                        format!(
                            "Node '{}' resolved to an unavailable local GraphCode repository",
                            seed.node_id
                        ),
                    )
                })?;
            let edge_filter = edge_filter_set(edge_types);

            let mut visited_node_ids: BTreeSet<String> = BTreeSet::new();
            let mut emitted_edges: BTreeSet<(String, String, String)> = BTreeSet::new();
            let mut queue: VecDeque<(String, usize)> = VecDeque::new();
            let mut scheduled: HashSet<String> = HashSet::new();
            let mut visited_edges = 0_usize;

            queue.push_back((seed.block_selector.clone(), 0));
            scheduled.insert(seed.block_selector.clone());

            while let Some((block_selector, distance)) = queue.pop_front() {
                if let Some(node_id) =
                    self.runtime_node_id_for_block(seed.repository_index, &block_selector)
                {
                    visited_node_ids.insert(node_id);
                }
                if distance >= depth {
                    continue;
                }

                let Some(block_id) = repository.navigator.resolve_selector(&block_selector) else {
                    continue;
                };
                let Some(source_node_id) =
                    self.runtime_node_id_for_block(seed.repository_index, &block_selector)
                else {
                    continue;
                };

                for edge in repository
                    .navigator
                    .graph()
                    .neighbors(block_id, GraphNeighborMode::Outgoing)
                {
                    visited_edges = visited_edges.saturating_add(1);
                    if edge_filter
                        .as_ref()
                        .is_some_and(|set| !set.contains(&edge.relation))
                    {
                        continue;
                    }
                    let target_selector = edge.to.to_string();
                    let Some(target_node_id) =
                        self.runtime_node_id_for_block(seed.repository_index, &target_selector)
                    else {
                        continue;
                    };
                    emitted_edges.insert((
                        source_node_id.clone(),
                        target_node_id,
                        edge.relation.clone(),
                    ));

                    if scheduled.insert(target_selector.clone()) {
                        queue.push_back((target_selector, distance.saturating_add(1)));
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

            return Ok(GraphQueryResult {
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
                selected_block_ids: Vec::new(),
                python_repo_name: None,
                python_result: None,
                python_summary: None,
                python_usage: None,
                python_limits: None,
                python_stdout: None,
                python_export_summary: None,
            });
        }

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
            selected_block_ids: Vec::new(),
            python_repo_name: None,
            python_result: None,
            python_summary: None,
            python_usage: None,
            python_limits: None,
            python_stdout: None,
            python_export_summary: None,
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
            selected_block_ids: Vec::new(),
            python_repo_name: None,
            python_result: None,
            python_summary: None,
            python_usage: None,
            python_limits: None,
            python_stdout: None,
            python_export_summary: None,
        }
    }
}
