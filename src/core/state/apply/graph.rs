use super::*;

impl AppState {
    pub(super) fn apply_graph_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::TaskGraphCreated {
                graph_id,
                project_id,
                name,
                description,
            } => {
                self.graphs.insert(
                    *graph_id,
                    TaskGraph {
                        id: *graph_id,
                        project_id: *project_id,
                        name: name.clone(),
                        description: description.clone(),
                        state: GraphState::Draft,
                        tasks: HashMap::new(),
                        dependencies: HashMap::<Uuid, HashSet<Uuid>>::new(),
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::TaskAddedToGraph { graph_id, task } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.tasks.insert(task.id, task.clone());
                    graph.dependencies.entry(task.id).or_default();
                    graph.updated_at = timestamp;
                }
                true
            }
            EventPayload::DependencyAdded {
                graph_id,
                from_task,
                to_task,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph
                        .dependencies
                        .entry(*to_task)
                        .or_default()
                        .insert(*from_task);
                    graph.updated_at = timestamp;
                }
                true
            }
            EventPayload::GraphTaskCheckAdded {
                graph_id,
                task_id,
                check,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.criteria.checks.push(check.clone());
                        graph.updated_at = timestamp;
                    }
                }
                true
            }
            EventPayload::ScopeAssigned {
                graph_id,
                task_id,
                scope,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    if let Some(task) = graph.tasks.get_mut(task_id) {
                        task.scope = Some(scope.clone());
                        graph.updated_at = timestamp;
                    }
                }
                true
            }
            EventPayload::TaskGraphValidated {
                graph_id,
                project_id: _,
                valid,
                issues: _,
            } => {
                if *valid {
                    if let Some(graph) = self.graphs.get_mut(graph_id) {
                        graph.state = GraphState::Validated;
                        graph.updated_at = timestamp;
                    }
                }
                true
            }
            EventPayload::TaskGraphLocked {
                graph_id,
                project_id: _,
            } => {
                if let Some(graph) = self.graphs.get_mut(graph_id) {
                    graph.state = GraphState::Locked;
                    graph.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskGraphDeleted {
                graph_id,
                project_id: _,
            } => {
                self.graphs.remove(graph_id);
                true
            }
            _ => false,
        }
    }
}
