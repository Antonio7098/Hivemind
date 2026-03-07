use super::*;

impl Registry {
    pub(crate) fn supported_runtime_descriptors() -> Vec<crate::adapters::RuntimeDescriptor> {
        let mut descriptors = runtime_descriptors().into_iter().collect::<Vec<_>>();
        if descriptors
            .iter()
            .all(|descriptor| descriptor.adapter_name != "native")
        {
            descriptors.push(crate::adapters::RuntimeDescriptor {
                adapter_name: "native",
                default_binary: "builtin-native",
                opencode_compatible: false,
                requires_binary: false,
                capabilities: &[
                    "native_loop",
                    "typed_tool_engine",
                    "schema_validated_tools",
                    "scope_policy_enforcement",
                    "deterministic_harness",
                    "provider_agnostic_contracts",
                ],
            });
        }
        descriptors
    }

    pub(crate) fn ensure_supported_runtime_adapter(
        adapter: &str,
        origin: &'static str,
    ) -> Result<()> {
        let supported = Self::supported_runtime_descriptors();
        if !supported
            .iter()
            .any(|descriptor| descriptor.adapter_name == adapter)
        {
            return Err(HivemindError::user(
                "invalid_runtime_adapter",
                format!(
                    "Unsupported runtime adapter '{adapter}'. Supported: {}",
                    supported
                        .iter()
                        .map(|descriptor| descriptor.adapter_name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                origin,
            ));
        }
        Ok(())
    }

    pub(crate) fn parse_runtime_env_pairs(
        env: &[String],
        origin: &'static str,
    ) -> Result<HashMap<String, String>> {
        let mut env_map = HashMap::new();
        for pair in env {
            let Some((k, v)) = pair.split_once('=') else {
                return Err(HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. Expected KEY=VALUE"),
                    origin,
                ));
            };
            if k.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. KEY cannot be empty"),
                    origin,
                ));
            }
            env_map.insert(k.to_string(), v.to_string());
        }
        Ok(env_map)
    }

    pub(crate) fn project_runtime_for_role(
        project: &Project,
        role: RuntimeRole,
    ) -> Option<ProjectRuntimeConfig> {
        Self::project_runtime_for_role_with_source(project, role).map(|(runtime, _)| runtime)
    }

    pub(crate) fn project_runtime_for_role_with_source(
        project: &Project,
        role: RuntimeRole,
    ) -> Option<(ProjectRuntimeConfig, RuntimeSelectionSource)> {
        match role {
            RuntimeRole::Worker => project
                .runtime_defaults
                .worker
                .clone()
                .map(|runtime| (runtime, RuntimeSelectionSource::ProjectDefault))
                .or_else(|| {
                    project
                        .runtime
                        .clone()
                        .map(|runtime| (runtime, RuntimeSelectionSource::ProjectDefault))
                }),
            RuntimeRole::Validator => project
                .runtime_defaults
                .validator
                .clone()
                .map(|runtime| (runtime, RuntimeSelectionSource::ProjectDefault)),
        }
    }

    pub(crate) fn task_runtime_override_for_role(
        task: &Task,
        role: RuntimeRole,
    ) -> Option<TaskRuntimeConfig> {
        match role {
            RuntimeRole::Worker => task
                .runtime_overrides
                .worker
                .clone()
                .or_else(|| task.runtime_override.clone()),
            RuntimeRole::Validator => task.runtime_overrides.validator.clone(),
        }
    }

    pub(crate) fn task_runtime_to_project_runtime(
        runtime: TaskRuntimeConfig,
        max_parallel_tasks: u16,
    ) -> ProjectRuntimeConfig {
        ProjectRuntimeConfig {
            adapter_name: runtime.adapter_name,
            binary_path: runtime.binary_path,
            model: runtime.model,
            args: runtime.args,
            env: runtime.env,
            timeout_ms: runtime.timeout_ms,
            max_parallel_tasks,
        }
    }

    pub(crate) fn max_parallel_from_defaults(defaults: &RuntimeRoleDefaults) -> Option<u16> {
        defaults.worker.as_ref().map(|cfg| cfg.max_parallel_tasks)
    }

    pub(crate) fn effective_runtime_for_task(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        role: RuntimeRole,
        origin: &'static str,
    ) -> Result<ProjectRuntimeConfig> {
        Self::effective_runtime_for_task_with_source(state, flow, task_id, role, origin)
            .map(|(runtime, _)| runtime)
    }

    pub(crate) fn effective_runtime_for_task_with_source(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        role: RuntimeRole,
        origin: &'static str,
    ) -> Result<(ProjectRuntimeConfig, RuntimeSelectionSource)> {
        let task = state.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_not_found",
                format!("Task '{task_id}' not found"),
                origin,
            )
        })?;

        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                origin,
            )
        })?;

        let flow_defaults = state
            .flow_runtime_defaults
            .get(&flow.id)
            .cloned()
            .unwrap_or_default();
        let project_defaults = project.runtime_defaults.clone();
        let global_defaults = state.global_runtime_defaults.clone();

        let max_parallel = match role {
            RuntimeRole::Worker | RuntimeRole::Validator => {
                Self::max_parallel_from_defaults(&flow_defaults)
                    .or_else(|| Self::max_parallel_from_defaults(&project_defaults))
                    .or_else(|| project.runtime.as_ref().map(|cfg| cfg.max_parallel_tasks))
                    .or_else(|| Self::max_parallel_from_defaults(&global_defaults))
                    .unwrap_or(1)
            }
        };

        if let Some(task_rt) = Self::task_runtime_override_for_role(task, role) {
            return Ok((
                Self::task_runtime_to_project_runtime(task_rt, max_parallel),
                RuntimeSelectionSource::TaskOverride,
            ));
        }
        if let Some(flow_rt) = match role {
            RuntimeRole::Worker => flow_defaults.worker,
            RuntimeRole::Validator => flow_defaults.validator,
        } {
            return Ok((flow_rt, RuntimeSelectionSource::FlowDefault));
        }
        if let Some((project_rt, source)) =
            Self::project_runtime_for_role_with_source(project, role)
        {
            return Ok((project_rt, source));
        }
        if let Some(global_rt) = match role {
            RuntimeRole::Worker => global_defaults.worker,
            RuntimeRole::Validator => global_defaults.validator,
        } {
            return Ok((global_rt, RuntimeSelectionSource::GlobalDefault));
        }

        Err(HivemindError::new(
            ErrorCategory::Runtime,
            "runtime_not_configured",
            format!("No runtime configured for role '{role:?}'"),
            origin,
        ))
    }
}
