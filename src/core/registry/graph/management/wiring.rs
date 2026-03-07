use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn add_graph_dependency(
        &self,
        graph_id: &str,
        from_task: &str,
        to_task: &str,
    ) -> Result<TaskGraph> {
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let from = Uuid::parse_str(from_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{from_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let to = Uuid::parse_str(to_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{to_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        if graph.state != GraphState::Draft {
            let locking_flow_id = state
                .flows
                .values()
                .filter(|flow| flow.graph_id == gid)
                .max_by_key(|flow| flow.updated_at)
                .map(|flow| flow.id);
            let message = locking_flow_id.map_or_else(
                || format!("Graph '{graph_id}' is immutable"),
                |flow_id| format!("Graph '{graph_id}' is immutable (locked by flow '{flow_id}')"),
            );

            let mut err = HivemindError::user(
                "graph_immutable",
                message,
                "registry:add_graph_dependency",
            )
            .with_hint(
                "Create a new graph if you need additional dependencies, or modify task execution in the existing flow",
            );
            if let Some(flow_id) = locking_flow_id {
                err = err.with_context("locking_flow_id", flow_id.to_string());
            }
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if !graph.tasks.contains_key(&from) || !graph.tasks.contains_key(&to) {
            let err = HivemindError::user(
                "task_not_in_graph",
                "One or more tasks are not in the graph",
                "registry:add_graph_dependency",
            )
            .with_hint("Ensure both task IDs were included when the graph was created");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if graph
            .dependencies
            .get(&to)
            .is_some_and(|deps| deps.contains(&from))
        {
            return Ok(graph);
        }

        let mut graph_for_check = graph.clone();
        graph_for_check.add_dependency(to, from).map_err(|e| {
            let err = HivemindError::user(
                "cycle_detected",
                e.to_string(),
                "registry:add_graph_dependency",
            )
            .with_hint("Remove one dependency in the cycle and try again");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            err
        })?;

        let event = Event::new(
            EventPayload::DependencyAdded {
                graph_id: gid,
                from_task: from,
                to_task: to,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:add_graph_dependency",
            )
        })?;

        self.get_graph(graph_id)
    }

    pub fn add_graph_task_check(
        &self,
        graph_id: &str,
        task_id: &str,
        check: crate::core::verification::CheckConfig,
    ) -> Result<TaskGraph> {
        let origin = "registry:add_graph_task_check";
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let tid = Uuid::parse_str(task_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        if graph.state != GraphState::Draft {
            let err = HivemindError::user(
                "graph_immutable",
                format!("Graph '{graph_id}' is immutable"),
                origin,
            )
            .with_hint("Checks can only be added to draft graphs");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }
        if !graph.tasks.contains_key(&tid) {
            let err = HivemindError::user(
                "task_not_in_graph",
                format!("Task '{task_id}' is not part of graph '{graph_id}'"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::GraphTaskCheckAdded {
                graph_id: gid,
                task_id: tid,
                check,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );
        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        self.get_graph(graph_id)
    }
}
