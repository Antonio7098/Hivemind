use super::*;

impl AppState {
    pub(super) fn apply_runtime_catalog_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::ProjectRuntimeConfigured {
                project_id,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    let configured = project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    );
                    project.runtime = Some(configured.clone());
                    project.runtime_defaults.worker = Some(configured);
                    project.updated_at = timestamp;
                }
                true
            }
            EventPayload::ProjectRuntimeRoleConfigured {
                project_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                if let Some(project) = self.projects.get_mut(project_id) {
                    let configured = project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    );
                    project
                        .runtime_defaults
                        .set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        project.runtime = Some(configured);
                    }
                    project.updated_at = timestamp;
                }
                true
            }
            EventPayload::GlobalRuntimeConfigured {
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
                max_parallel_tasks,
            } => {
                self.global_runtime_defaults.set(
                    *role,
                    Some(project_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                        *max_parallel_tasks,
                    )),
                );
                true
            }
            EventPayload::TaskRuntimeConfigured {
                task_id,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    let configured = task_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                    );
                    task.runtime_override = Some(configured.clone());
                    task.runtime_overrides.worker = Some(configured);
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRuntimeRoleConfigured {
                task_id,
                role,
                adapter_name,
                binary_path,
                model,
                args,
                env,
                timeout_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    let configured = task_runtime_config(
                        adapter_name,
                        binary_path,
                        model,
                        args,
                        env,
                        *timeout_ms,
                    );
                    task.runtime_overrides.set(*role, Some(configured.clone()));
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = Some(configured);
                    }
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRuntimeCleared { task_id } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_override = None;
                    task.runtime_overrides.worker = None;
                    task.updated_at = timestamp;
                }
                true
            }
            EventPayload::TaskRuntimeRoleCleared { task_id, role } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    task.runtime_overrides.set(*role, None);
                    if *role == RuntimeRole::Worker {
                        task.runtime_override = None;
                    }
                    task.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
