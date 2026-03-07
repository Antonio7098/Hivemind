use super::*;

impl Registry {
    /// Creates a new task in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn create_task(
        &self,
        project_id_or_name: &str,
        title: &str,
        description: Option<&str>,
        scope: Option<Scope>,
    ) -> Result<Task> {
        let project = self
            .get_project(project_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if title.trim().is_empty() {
            let err = HivemindError::user(
                "invalid_task_title",
                "Task title cannot be empty",
                "registry:create_task",
            )
            .with_hint("Provide a non-empty task title");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let task_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskCreated {
                id: task_id,
                project_id: project.id,
                title: title.to_string(),
                description: description.map(String::from),
                scope,
            },
            CorrelationIds::for_task(project.id, task_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_task")
        })?;

        self.get_task(&task_id.to_string())
    }

    /// Lists tasks in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn list_tasks(
        &self,
        project_id_or_name: &str,
        state_filter: Option<TaskState>,
    ) -> Result<Vec<Task>> {
        let project = self.get_project(project_id_or_name)?;
        let state = self.state()?;

        let mut tasks: Vec<_> = state
            .tasks
            .into_values()
            .filter(|t| t.project_id == project.id)
            .filter(|t| state_filter.is_none_or(|s| t.state == s))
            .collect();

        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    /// Gets a task by ID.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:get_task",
            )
        })?;

        let state = self.state()?;
        state.tasks.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "task_not_found",
                format!("Task '{task_id}' not found"),
                "registry:get_task",
            )
            .with_hint("Use 'hivemind task list <project>' to see available tasks")
        })
    }

    /// Updates a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn update_task(
        &self,
        task_id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<Task> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if let Some(new_title) = title {
            if new_title.trim().is_empty() {
                let err = HivemindError::user(
                    "invalid_task_title",
                    "Task title cannot be empty",
                    "registry:update_task",
                )
                .with_hint("Provide a non-empty task title");
                self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
                return Err(err);
            }
        }

        let title = title.filter(|t| *t != task.title);
        let description = description.filter(|d| task.description.as_deref() != Some(*d));

        if title.is_none() && description.is_none() {
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskUpdated {
                id: task.id,
                title: title.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:update_task")
        })?;

        self.get_task(task_id)
    }

    /// Deletes a task.
    ///
    /// # Errors
    /// Returns an error if the task is referenced by a graph or flow.
    pub fn delete_task(&self, task_id: &str) -> Result<Uuid> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        if state
            .graphs
            .values()
            .any(|graph| graph.tasks.contains_key(&task.id))
        {
            let err = HivemindError::user(
                "task_in_graph",
                "Task is referenced by a graph",
                "registry:delete_task",
            )
            .with_hint("Remove the graph(s) that reference this task first");
            self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
            return Err(err);
        }

        if state
            .flows
            .values()
            .any(|flow| flow.task_executions.contains_key(&task.id))
        {
            let err = HivemindError::user(
                "task_in_flow",
                "Task is referenced by a flow",
                "registry:delete_task",
            )
            .with_hint("Delete the flow(s) that reference this task first");
            self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::TaskDeleted {
                task_id: task.id,
                project_id: task.project_id,
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:delete_task")
        })?;

        Ok(task.id)
    }
}
