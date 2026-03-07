use super::*;

impl Registry {
    pub fn verify_run(&self, task_id: &str) -> Result<TaskFlow> {
        let origin = "registry:verify_run";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;
        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Verifying {
            return Err(HivemindError::user(
                "task_not_verifying",
                "Task is not in verifying state",
                origin,
            )
            .with_hint(
                "Complete the task execution first, or run `hivemind flow tick <flow-id>`",
            ));
        }

        self.process_verifying_task(&flow.id.to_string(), id)
    }
}
