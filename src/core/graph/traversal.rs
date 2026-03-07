use super::*;

impl TaskGraph {
    /// Checks if the graph contains a cycle using DFS.
    pub(crate) fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for task_id in self.tasks.keys() {
            if self.has_cycle_util(*task_id, &mut visited, &mut rec_stack) {
                return true;
            }
        }
        false
    }

    fn has_cycle_util(
        &self,
        node: Uuid,
        visited: &mut HashSet<Uuid>,
        rec_stack: &mut HashSet<Uuid>,
    ) -> bool {
        if rec_stack.contains(&node) {
            return true;
        }
        if visited.contains(&node) {
            return false;
        }

        visited.insert(node);
        rec_stack.insert(node);

        if let Some(deps) = self.dependencies.get(&node) {
            for dep in deps {
                if self.has_cycle_util(*dep, visited, rec_stack) {
                    return true;
                }
            }
        }

        rec_stack.remove(&node);
        false
    }

    /// Returns tasks in topological order (dependencies first).
    #[must_use]
    pub fn topological_order(&self) -> Vec<Uuid> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();

        for task_id in self.tasks.keys() {
            self.topological_visit(*task_id, &mut visited, &mut result);
        }

        result
    }

    fn topological_visit(&self, node: Uuid, visited: &mut HashSet<Uuid>, result: &mut Vec<Uuid>) {
        if visited.contains(&node) {
            return;
        }

        visited.insert(node);

        if let Some(deps) = self.dependencies.get(&node) {
            for dep in deps {
                self.topological_visit(*dep, visited, result);
            }
        }

        result.push(node);
    }

    /// Gets tasks that have no dependencies (entry points).
    #[must_use]
    pub fn root_tasks(&self) -> Vec<Uuid> {
        self.tasks
            .keys()
            .filter(|id| self.dependencies.get(*id).is_none_or(HashSet::is_empty))
            .copied()
            .collect()
    }

    /// Gets tasks that depend on a given task.
    #[must_use]
    pub fn dependents(&self, task_id: Uuid) -> Vec<Uuid> {
        self.dependencies
            .iter()
            .filter(|(_, deps)| deps.contains(&task_id))
            .map(|(id, _)| *id)
            .collect()
    }
}
