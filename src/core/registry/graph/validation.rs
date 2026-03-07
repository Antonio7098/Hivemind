use super::*;

impl Registry {
    pub(crate) fn validate_graph_issues(graph: &TaskGraph) -> Vec<String> {
        fn has_cycle(graph: &TaskGraph) -> bool {
            use std::collections::HashSet;

            fn visit(
                graph: &TaskGraph,
                node: Uuid,
                visited: &mut HashSet<Uuid>,
                stack: &mut HashSet<Uuid>,
            ) -> bool {
                if stack.contains(&node) {
                    return true;
                }
                if visited.contains(&node) {
                    return false;
                }

                visited.insert(node);
                stack.insert(node);

                if let Some(deps) = graph.dependencies.get(&node) {
                    for dep in deps {
                        if visit(graph, *dep, visited, stack) {
                            return true;
                        }
                    }
                }

                stack.remove(&node);
                false
            }

            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            for node in graph.tasks.keys() {
                if visit(graph, *node, &mut visited, &mut stack) {
                    return true;
                }
            }
            false
        }

        if graph.tasks.is_empty() {
            return vec!["Graph must contain at least one task".to_string()];
        }

        for (task_id, deps) in &graph.dependencies {
            if !graph.tasks.contains_key(task_id) {
                return vec![format!("Task not found: {task_id}")];
            }
            for dep in deps {
                if !graph.tasks.contains_key(dep) {
                    return vec![format!("Task not found: {dep}")];
                }
            }
        }

        if has_cycle(graph) {
            return vec!["Cycle detected in task dependencies".to_string()];
        }

        Vec::new()
    }

    pub fn validate_graph(&self, graph_id: &str) -> Result<GraphValidationResult> {
        let graph = self.get_graph(graph_id)?;
        let issues = Self::validate_graph_issues(&graph);
        Ok(GraphValidationResult {
            graph_id: graph.id,
            valid: issues.is_empty(),
            issues,
        })
    }
}
