use super::support::{
    edge_filter_set, normalize_graph_path, request_max_results, truncate_node_ids,
};
use super::*;
use std::collections::{BTreeSet, HashSet, VecDeque};

impl GraphQueryIndex {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
}
