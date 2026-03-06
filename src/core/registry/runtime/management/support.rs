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

    #[test]
    fn ensure_supported_runtime_adapter_accepts_native() {
        Registry::ensure_supported_runtime_adapter("native", "runtime:test")
            .expect("native adapter should be supported");
    }

    #[test]
    fn supported_runtime_descriptors_include_native() {
        let descriptors = Registry::supported_runtime_descriptors();
        assert!(descriptors.iter().any(|descriptor| {
            descriptor.adapter_name == "native"
                && descriptor.default_binary == "builtin-native"
                && !descriptor.requires_binary
        }));
    }
}
