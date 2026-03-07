use super::*;

impl Registry {
    pub(crate) fn flow_for_task(
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<TaskFlow> {
        state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&task_id))
            .max_by_key(|f| (f.updated_at, f.id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user("task_not_in_flow", "Task is not part of any flow", origin)
            })
    }

    pub(crate) fn project_for_flow<'a>(
        flow: &TaskFlow,
        state: &'a AppState,
    ) -> Result<&'a Project> {
        state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:worktree_manager_for_flow",
            )
        })
    }
}
