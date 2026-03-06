use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub fn task_runtime_set(
        &self,
        task_id: &str,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
    ) -> Result<Task> {
        self.task_runtime_set_role(
            task_id,
            RuntimeRole::Worker,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn task_runtime_set_role(
        &self,
        task_id: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
    ) -> Result<Task> {
        let task = self.get_task(task_id)?;
        Self::ensure_supported_runtime_adapter(adapter, "registry:task_runtime_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:task_runtime_set")?;

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::TaskRuntimeConfigured {
                    task_id: task.id,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::TaskRuntimeRoleConfigured {
                    task_id: task.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
        };
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:task_runtime_set",
            )
        })?;
        self.get_task(task_id)
    }

    pub fn task_runtime_clear(&self, task_id: &str) -> Result<Task> {
        self.task_runtime_clear_role(task_id, RuntimeRole::Worker)
    }

    pub fn task_runtime_clear_role(&self, task_id: &str, role: RuntimeRole) -> Result<Task> {
        let task = self.get_task(task_id)?;
        let already_cleared = match role {
            RuntimeRole::Worker => {
                task.runtime_override.is_none() && task.runtime_overrides.worker.is_none()
            }
            RuntimeRole::Validator => task.runtime_overrides.validator.is_none(),
        };
        if already_cleared {
            return Ok(task);
        }

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::TaskRuntimeCleared { task_id: task.id },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::TaskRuntimeRoleCleared {
                    task_id: task.id,
                    role,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
        };
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:task_runtime_clear",
            )
        })?;
        self.get_task(task_id)
    }

    pub fn task_set_run_mode(&self, task_id: &str, mode: RunMode) -> Result<Task> {
        let task = self.get_task(task_id)?;
        if task.run_mode == mode {
            return Ok(task);
        }

        self.append_event(
            Event::new(
                EventPayload::TaskRunModeSet {
                    task_id: task.id,
                    mode,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            "registry:task_set_run_mode",
        )?;
        if mode == RunMode::Auto {
            let state = self.state()?;
            if let Some(flow) = state
                .flows
                .values()
                .filter(|flow| flow.task_executions.contains_key(&task.id))
                .max_by_key(|flow| flow.updated_at)
                .filter(|flow| flow.run_mode == RunMode::Auto && flow.state == FlowState::Running)
            {
                let _ = self.auto_progress_flow(&flow.id.to_string());
            }
        }
        self.get_task(task_id)
    }
}
