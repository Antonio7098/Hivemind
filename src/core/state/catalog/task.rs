use super::*;

impl AppState {
    pub(super) fn apply_task_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::TaskCreated {
                id,
                project_id,
                title,
                description,
                scope,
            } => {
                self.tasks.insert(
                    *id,
                    Task {
                        id: *id,
                        project_id: *project_id,
                        title: title.clone(),
                        description: description.clone(),
                        scope: scope.clone(),
                        runtime_override: None,
                        runtime_overrides: TaskRuntimeRoleOverrides::default(),
                        run_mode: RunMode::Auto,
                        state: TaskState::Open,
                        created_at: timestamp,
                        updated_at: timestamp,
                    },
                );
                true
            }
            EventPayload::TaskUpdated {
                id,
                title,
                description,
            } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    if let Some(t) = title {
                        t.clone_into(&mut task.title);
                    }
                    if let Some(d) = description {
                        task.description = Some(d.clone());
                    }
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRunModeSet { task_id, mode } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.run_mode = *mode;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskClosed { id, reason: _ } => {
                if let Some(task) = self.tasks.get_mut(id) {
                    task.state = TaskState::Closed;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskDeleted {
                task_id,
                project_id: _,
            } => {
                self.tasks.remove(task_id);
                true
            }
            _ => false,
        }
    }
}
