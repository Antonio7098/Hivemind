use super::super::support::normalize_graph_path;
use super::*;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub(super) struct ResolvedRuntimeNode {
    pub(super) node_id: String,
    pub(super) repository_index: usize,
    pub(super) block_selector: String,
}

pub(super) fn collect_nodes(
    nodes: &BTreeMap<String, IndexedNode>,
    node_ids: &BTreeSet<String>,
) -> Vec<GraphQueryNode> {
    node_ids
        .iter()
        .filter_map(|node_id| {
            nodes.get(node_id).map(|node| GraphQueryNode {
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

impl GraphQueryIndex {
    pub(crate) fn resolve_node_id(&self, input: &str) -> Result<String, GraphQueryError> {
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

    pub(super) fn resolve_runtime_node(
        &self,
        input: &str,
    ) -> Result<ResolvedRuntimeNode, GraphQueryError> {
        let node_id = self.resolve_node_id(input)?;
        let repository_index = self
            .node_to_repository_index
            .get(&node_id)
            .copied()
            .ok_or_else(|| {
                GraphQueryError::new(
                    "graph_query_runtime_unavailable",
                    format!("Node '{node_id}' is not available in the local GraphCode runtime"),
                )
            })?;
        let block_selector = self
            .node_to_block_selector
            .get(&node_id)
            .cloned()
            .ok_or_else(|| {
                GraphQueryError::new(
                    "graph_query_runtime_unavailable",
                    format!("Node '{node_id}' is missing a local GraphCode selector"),
                )
            })?;
        Ok(ResolvedRuntimeNode {
            node_id,
            repository_index,
            block_selector,
        })
    }

    pub(super) fn runtime_node_id_for_block(
        &self,
        repository_index: usize,
        block_selector: &str,
    ) -> Option<String> {
        self.repositories
            .get(repository_index)?
            .block_to_node_id
            .get(block_selector)
            .cloned()
    }
}
