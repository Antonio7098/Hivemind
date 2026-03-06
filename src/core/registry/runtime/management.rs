use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set(
        &self,
        id_or_name: &str,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        self.project_runtime_set_role(
            id_or_name,
            RuntimeRole::Worker,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
            max_parallel_tasks,
        )
    }

    pub(crate) fn ensure_supported_runtime_adapter(
        adapter: &str,
        origin: &'static str,
    ) -> Result<()> {
        if !SUPPORTED_ADAPTERS.contains(&adapter) {
            return Err(HivemindError::user(
                "invalid_runtime_adapter",
                format!(
                    "Unsupported runtime adapter '{adapter}'. Supported: {}",
                    SUPPORTED_ADAPTERS.join(", ")
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
            RuntimeRole::Worker => Self::max_parallel_from_defaults(&flow_defaults)
                .or_else(|| Self::max_parallel_from_defaults(&project_defaults))
                .or_else(|| project.runtime.as_ref().map(|cfg| cfg.max_parallel_tasks))
                .or_else(|| Self::max_parallel_from_defaults(&global_defaults))
                .unwrap_or(1),
            RuntimeRole::Validator => Self::max_parallel_from_defaults(&flow_defaults)
                .or_else(|| Self::max_parallel_from_defaults(&project_defaults))
                .or_else(|| project.runtime.as_ref().map(|cfg| cfg.max_parallel_tasks))
                .or_else(|| Self::max_parallel_from_defaults(&global_defaults))
                .unwrap_or(1),
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

    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set_role(
        &self,
        id_or_name: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if max_parallel_tasks == 0 {
            let err = HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:project_runtime_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if let Err(err) =
            Self::ensure_supported_runtime_adapter(adapter, "registry:project_runtime_set")
        {
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let env_map = match Self::parse_runtime_env_pairs(env, "registry:project_runtime_set") {
            Ok(parsed) => parsed,
            Err(err) => {
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        };

        let desired = crate::core::state::ProjectRuntimeConfig {
            adapter_name: adapter.to_string(),
            binary_path: binary_path.to_string(),
            model: model.clone(),
            args: args.to_vec(),
            env: env_map.clone(),
            timeout_ms,
            max_parallel_tasks,
        };
        let current = Self::project_runtime_for_role(&project, role);
        if current.as_ref() == Some(&desired) {
            return Ok(project);
        }

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::ProjectRuntimeConfigured {
                    project_id: project.id,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::ProjectRuntimeRoleConfigured {
                    project_id: project.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
        };

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:project_runtime_set",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn runtime_defaults_set(
        &self,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<()> {
        if max_parallel_tasks == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:runtime_defaults_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher"));
        }
        Self::ensure_supported_runtime_adapter(adapter, "registry:runtime_defaults_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:runtime_defaults_set")?;

        self.append_event(
            Event::new(
                EventPayload::GlobalRuntimeConfigured {
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::none(),
            ),
            "registry:runtime_defaults_set",
        )
    }

    #[must_use]
    pub fn runtime_list(&self) -> Vec<RuntimeListEntry> {
        runtime_descriptors()
            .into_iter()
            .map(|d| RuntimeListEntry {
                adapter_name: d.adapter_name.to_string(),
                default_binary: d.default_binary.to_string(),
                available: if d.requires_binary {
                    Self::binary_available(d.default_binary)
                } else {
                    true
                },
                opencode_compatible: d.opencode_compatible,
                capabilities: d
                    .capabilities
                    .iter()
                    .map(|capability| (*capability).to_string())
                    .collect(),
            })
            .collect()
    }

    pub fn runtime_health(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<RuntimeHealthStatus> {
        self.runtime_health_with_role(project, task_id, None, RuntimeRole::Worker)
    }

    #[allow(clippy::too_many_lines)]
    pub fn runtime_health_with_role(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
        flow_id: Option<&str>,
        role: RuntimeRole,
    ) -> Result<RuntimeHealthStatus> {
        if let Some(flow_id) = flow_id {
            let flow = self.get_flow(flow_id)?;
            let state = self.state()?;
            let flow_defaults = state
                .flow_runtime_defaults
                .get(&flow.id)
                .cloned()
                .unwrap_or_default();
            let runtime = match role {
                RuntimeRole::Worker => flow_defaults.worker,
                RuntimeRole::Validator => flow_defaults.validator,
            }
            .map(|runtime| (runtime, RuntimeSelectionSource::FlowDefault))
            .or_else(|| {
                state
                    .projects
                    .get(&flow.project_id)
                    .and_then(|project| Self::project_runtime_for_role_with_source(project, role))
            })
            .or_else(|| match role {
                RuntimeRole::Worker => state
                    .global_runtime_defaults
                    .worker
                    .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault)),
                RuntimeRole::Validator => state
                    .global_runtime_defaults
                    .validator
                    .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault)),
            })
            .ok_or_else(|| {
                HivemindError::new(
                    ErrorCategory::Runtime,
                    "runtime_not_configured",
                    "Flow has no effective runtime configured for this role",
                    "registry:runtime_health",
                )
            })?;
            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("flow:{flow_id}:{role:?}")),
                Some(runtime.1),
            ));
        }

        if let Some(task_id) = task_id {
            let task_uuid = Uuid::parse_str(task_id).map_err(|_| {
                HivemindError::user(
                    "invalid_task_id",
                    format!("'{task_id}' is not a valid task ID"),
                    "registry:runtime_health",
                )
            })?;
            let state = self.state()?;
            let flow = state
                .flows
                .values()
                .filter(|f| f.task_executions.contains_key(&task_uuid))
                .max_by_key(|f| f.updated_at)
                .cloned()
                .ok_or_else(|| {
                    HivemindError::user(
                        "task_not_in_flow",
                        "Task is not part of any flow",
                        "registry:runtime_health",
                    )
                })?;
            let runtime = Self::effective_runtime_for_task_with_source(
                &state,
                &flow,
                task_uuid,
                role,
                "registry:runtime_health",
            )?;

            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("task:{task_id}:{role:?}")),
                Some(runtime.1),
            ));
        }

        if let Some(project_id_or_name) = project {
            let project = self.get_project(project_id_or_name)?;
            let runtime =
                Self::project_runtime_for_role_with_source(&project, role).ok_or_else(|| {
                    HivemindError::new(
                        ErrorCategory::Runtime,
                        "runtime_not_configured",
                        "Project has no runtime configured",
                        "registry:runtime_health",
                    )
                })?;
            return Ok(Self::health_for_runtime(
                &runtime.0,
                Some(format!("project:{project_id_or_name}:{role:?}")),
                Some(runtime.1),
            ));
        }

        Ok(RuntimeHealthStatus {
            adapter_name: "all".to_string(),
            binary_path: "builtin-defaults".to_string(),
            healthy: self.runtime_list().iter().all(|r| r.available),
            capabilities: vec!["catalog".to_string()],
            selection_source: None,
            target: None,
            details: Some(
                self.runtime_list()
                    .into_iter()
                    .map(|r| {
                        format!(
                            "{}={} ({})",
                            r.adapter_name,
                            if r.available { "ok" } else { "missing" },
                            r.default_binary
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        })
    }

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

    pub(crate) fn binary_available(binary: &str) -> bool {
        if binary.contains('/') {
            let path = PathBuf::from(binary);
            return path.exists();
        }

        std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(binary);
                candidate.exists() && candidate.is_file()
            })
        })
    }

    pub(crate) fn health_for_runtime(
        runtime: &ProjectRuntimeConfig,
        target: Option<String>,
        selection_source: Option<RuntimeSelectionSource>,
    ) -> RuntimeHealthStatus {
        match Self::build_runtime_adapter(runtime.clone()) {
            Ok(mut adapter) => match adapter.initialize() {
                Ok(()) => RuntimeHealthStatus {
                    adapter_name: runtime.adapter_name.clone(),
                    binary_path: runtime.binary_path.clone(),
                    healthy: true,
                    capabilities: Self::runtime_capabilities_for_adapter(&runtime.adapter_name),
                    selection_source,
                    target,
                    details: None,
                },
                Err(e) => RuntimeHealthStatus {
                    adapter_name: runtime.adapter_name.clone(),
                    binary_path: runtime.binary_path.clone(),
                    healthy: false,
                    capabilities: Self::runtime_capabilities_for_adapter(&runtime.adapter_name),
                    selection_source,
                    target,
                    details: Some(format!("{}: {}", e.code, e.message)),
                },
            },
            Err(e) => RuntimeHealthStatus {
                adapter_name: runtime.adapter_name.clone(),
                binary_path: runtime.binary_path.clone(),
                healthy: false,
                capabilities: Self::runtime_capabilities_for_adapter(&runtime.adapter_name),
                selection_source,
                target,
                details: Some(format!("{}: {}", e.code, e.message)),
            },
        }
    }

    pub(crate) fn build_runtime_adapter(
        runtime: ProjectRuntimeConfig,
    ) -> Result<SelectedRuntimeAdapter> {
        let mut runtime = runtime;
        Self::prepare_runtime_environment(&mut runtime, "registry:build_runtime_adapter")?;
        let timeout = Duration::from_millis(runtime.timeout_ms);
        match runtime.adapter_name.as_str() {
            "opencode" => {
                let mut cfg = OpenCodeConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model.clone().or(cfg.model);
                cfg.base.args = runtime.args;
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::OpenCode(
                    crate::adapters::opencode::OpenCodeAdapter::new(cfg),
                ))
            }
            "codex" => {
                let mut cfg = CodexConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = if runtime.args.is_empty() {
                    CodexConfig::default().base.args
                } else {
                    runtime.args
                };
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::Codex(CodexAdapter::new(cfg)))
            }
            "claude-code" => {
                let mut cfg = ClaudeCodeConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = if runtime.args.is_empty() {
                    ClaudeCodeConfig::default().base.args
                } else {
                    runtime.args
                };
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::ClaudeCode(ClaudeCodeAdapter::new(
                    cfg,
                )))
            }
            "kilo" => {
                let mut cfg = KiloConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = runtime.args;
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::Kilo(KiloAdapter::new(cfg)))
            }
            "native" => {
                let mut cfg = NativeAdapterConfig::new();
                cfg.model_name = runtime
                    .model
                    .clone()
                    .unwrap_or_else(|| cfg.model_name.clone());
                if let Some(provider) = runtime.env.get("HIVEMIND_NATIVE_PROVIDER") {
                    cfg.provider_name.clone_from(provider);
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS") {
                    let normalized = raw.trim().to_ascii_lowercase();
                    cfg.native.capture_full_payloads =
                        matches!(normalized.as_str(), "1" | "true" | "yes" | "on");
                }
                cfg.base.binary_path = PathBuf::from(runtime.binary_path);
                cfg.base.timeout = timeout;
                cfg.base.env = runtime.env;
                cfg.base.args = runtime.args;
                cfg.scripted_directives = if cfg.base.args.is_empty() {
                    cfg.scripted_directives.clone()
                } else {
                    cfg.base.args.clone()
                };
                Ok(SelectedRuntimeAdapter::Native(NativeRuntimeAdapter::new(
                    cfg,
                )))
            }
            _ => Err(HivemindError::user(
                "unsupported_runtime",
                format!("Unsupported runtime adapter '{}'", runtime.adapter_name),
                "registry:build_runtime_adapter",
            )),
        }
    }

    pub(crate) fn parse_global_parallel_limit(raw: Option<String>) -> Result<u16> {
        let Some(raw) = raw else {
            return Ok(u16::MAX);
        };

        let parsed = raw.parse::<u16>().map_err(|_| {
            HivemindError::user(
                "invalid_global_parallel_limit",
                format!(
                    "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be a positive integer, got '{raw}'"
                ),
                "registry:tick_flow",
            )
        })?;

        if parsed == 0 {
            return Err(HivemindError::user(
                "invalid_global_parallel_limit",
                "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be at least 1",
                "registry:tick_flow",
            ));
        }

        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_runtime_env_pairs_accepts_key_value_entries() {
        let parsed = Registry::parse_runtime_env_pairs(
            &["FOO=bar".to_string(), "BAZ=qux".to_string()],
            "runtime:test",
        )
        .expect("valid env pairs");

        assert_eq!(parsed.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(parsed.get("BAZ").map(String::as_str), Some("qux"));
    }

    #[test]
    fn parse_global_parallel_limit_rejects_zero() {
        let err = Registry::parse_global_parallel_limit(Some("0".to_string())).unwrap_err();
        assert!(err.code.contains("invalid_global_parallel_limit"));
    }
}
