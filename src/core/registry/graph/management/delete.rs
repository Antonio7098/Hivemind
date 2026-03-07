use super::*;

impl Registry {
    /// Deletes a graph.
    ///
    /// # Errors
    /// Returns an error if the graph is referenced by any flow.
    pub fn delete_graph(&self, graph_id: &str) -> Result<Uuid> {
        let graph = self
            .get_graph(graph_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        if state.flows.values().any(|flow| flow.graph_id == graph.id) {
            let err = HivemindError::user(
                "graph_in_use",
                "Graph is referenced by an existing flow",
                "registry:delete_graph",
            )
            .with_hint("Delete the flow(s) that reference this graph first");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::TaskGraphDeleted {
                graph_id: graph.id,
                project_id: graph.project_id,
            },
            CorrelationIds::for_graph(graph.project_id, graph.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:delete_graph",
            )
        })?;

        Ok(graph.id)
    }
}
