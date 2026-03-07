use super::*;

impl TaskGraph {
    /// Adds a task to the graph.
    ///
    /// # Errors
    /// Returns an error if the graph is locked.
    pub fn add_task(&mut self, task: GraphTask) -> Result<Uuid, GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        let id = task.id;
        self.tasks.insert(id, task);
        self.dependencies.insert(id, HashSet::new());
        self.updated_at = Utc::now();
        Ok(id)
    }

    /// Adds a dependency between tasks.
    ///
    /// # Errors
    /// Returns an error if the graph is locked, tasks don't exist, or would create a cycle.
    pub fn add_dependency(&mut self, from: Uuid, to: Uuid) -> Result<(), GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        if !self.tasks.contains_key(&from) {
            return Err(GraphError::TaskNotFound(from));
        }
        if !self.tasks.contains_key(&to) {
            return Err(GraphError::TaskNotFound(to));
        }
        if from == to {
            return Err(GraphError::CycleDetected);
        }

        self.dependencies.entry(from).or_default().insert(to);
        if self.has_cycle() {
            self.dependencies.entry(from).or_default().remove(&to);
            return Err(GraphError::CycleDetected);
        }

        self.updated_at = Utc::now();
        Ok(())
    }

    /// Sets scope for a task.
    ///
    /// # Errors
    /// Returns an error if the graph is locked or task doesn't exist.
    pub fn set_scope(&mut self, task_id: Uuid, scope: Scope) -> Result<(), GraphError> {
        if !self.is_modifiable() {
            return Err(GraphError::GraphLocked);
        }

        let task = self
            .tasks
            .get_mut(&task_id)
            .ok_or(GraphError::TaskNotFound(task_id))?;
        task.scope = Some(scope);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Validates the graph and transitions to Validated state.
    ///
    /// # Errors
    /// Returns an error if validation fails.
    pub fn validate(&mut self) -> Result<(), GraphError> {
        if self.state != GraphState::Draft {
            return Err(GraphError::InvalidStateTransition);
        }
        if self.tasks.is_empty() {
            return Err(GraphError::EmptyGraph);
        }
        if self.has_cycle() {
            return Err(GraphError::CycleDetected);
        }

        for (task_id, deps) in &self.dependencies {
            if !self.tasks.contains_key(task_id) {
                return Err(GraphError::TaskNotFound(*task_id));
            }
            for dep in deps {
                if !self.tasks.contains_key(dep) {
                    return Err(GraphError::TaskNotFound(*dep));
                }
            }
        }

        self.state = GraphState::Validated;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Locks the graph (immutable after this).
    ///
    /// # Errors
    /// Returns an error if the graph is not validated.
    pub fn lock(&mut self) -> Result<(), GraphError> {
        if self.state != GraphState::Validated {
            return Err(GraphError::InvalidStateTransition);
        }

        self.state = GraphState::Locked;
        self.updated_at = Utc::now();
        Ok(())
    }
}
