use super::*;

impl Registry {
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
        let resume_session_id = runtime.env.remove("HIVEMIND_RUNTIME_RESUME_SESSION_ID");
        match runtime.adapter_name.as_str() {
            "opencode" => {
                let mut cfg = OpenCodeConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model.clone().or(cfg.model);
                cfg.base.args = runtime.args;
                cfg.base.env = runtime.env;
                cfg.base.resume_session_id.clone_from(&resume_session_id);
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
                cfg.base.resume_session_id.clone_from(&resume_session_id);
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
                cfg.base.resume_session_id.clone_from(&resume_session_id);
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
