use super::support::request_max_results;
use super::*;

mod resolve;
#[cfg(test)]
mod tests;
mod variants;

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
}
