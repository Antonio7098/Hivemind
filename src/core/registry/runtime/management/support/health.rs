use super::*;

impl Registry {
    fn parse_native_agent_mode(
        raw: &str,
        fallback: crate::native::AgentMode,
    ) -> crate::native::AgentMode {
        match raw.trim().to_ascii_lowercase().as_str() {
            "planner" => crate::native::AgentMode::Planner,
            "freeflow" => crate::native::AgentMode::Freeflow,
            "task_executor" | "task-executor" | "executor" => {
                crate::native::AgentMode::TaskExecutor
            }
            _ => fallback,
        }
    }

    fn parse_native_runtime_args(cfg: &mut NativeAdapterConfig, args: &[String]) -> Vec<String> {
        let mut scripted = Vec::new();
        let mut index = 0usize;
        while index < args.len() {
            let arg = &args[index];
            let mut handled = true;
            if let Some(raw) = arg.strip_prefix("--max-turns=") {
                if let Ok(parsed) = raw.trim().parse::<u32>() {
                    cfg.native.max_turns = parsed.max(1);
                }
            } else if arg == "--max-turns" {
                index += 1;
                if let Some(raw) = args.get(index) {
                    if let Ok(parsed) = raw.trim().parse::<u32>() {
                        cfg.native.max_turns = parsed.max(1);
                    }
                }
            } else if let Some(raw) = arg.strip_prefix("--token-budget=") {
                if let Ok(parsed) = raw.trim().parse::<usize>() {
                    cfg.native.token_budget = parsed;
                }
            } else if arg == "--token-budget" {
                index += 1;
                if let Some(raw) = args.get(index) {
                    if let Ok(parsed) = raw.trim().parse::<usize>() {
                        cfg.native.token_budget = parsed;
                    }
                }
            } else if let Some(raw) = arg.strip_prefix("--prompt-headroom=") {
                if let Ok(parsed) = raw.trim().parse::<usize>() {
                    cfg.native.prompt_headroom = parsed;
                }
            } else if arg == "--prompt-headroom" {
                index += 1;
                if let Some(raw) = args.get(index) {
                    if let Ok(parsed) = raw.trim().parse::<usize>() {
                        cfg.native.prompt_headroom = parsed;
                    }
                }
            } else if let Some(raw) = arg.strip_prefix("--agent-mode=") {
                cfg.native.agent_mode = Self::parse_native_agent_mode(raw, cfg.native.agent_mode);
            } else if arg == "--agent-mode" {
                index += 1;
                if let Some(raw) = args.get(index) {
                    cfg.native.agent_mode =
                        Self::parse_native_agent_mode(raw, cfg.native.agent_mode);
                }
            } else if let Some(raw) = arg.strip_prefix("--timeout-budget-ms=") {
                if let Ok(parsed) = raw.trim().parse::<u64>() {
                    cfg.native.timeout_budget = Duration::from_millis(parsed);
                }
            } else if arg == "--timeout-budget-ms" {
                index += 1;
                if let Some(raw) = args.get(index) {
                    if let Ok(parsed) = raw.trim().parse::<u64>() {
                        cfg.native.timeout_budget = Duration::from_millis(parsed);
                    }
                }
            } else if arg == "--capture-full-payloads" {
                cfg.native.capture_full_payloads = true;
            } else {
                handled = false;
            }

            if !handled {
                scripted.push(arg.clone());
            }
            index += 1;
        }
        scripted
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
                let runtime_args = runtime.args.clone();
                cfg.model_name = runtime
                    .model
                    .clone()
                    .unwrap_or_else(|| cfg.model_name.clone());
                cfg.native.timeout_budget = timeout;
                if let Some(provider) = runtime.env.get("HIVEMIND_NATIVE_PROVIDER") {
                    cfg.provider_name.clone_from(provider);
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS") {
                    let normalized = raw.trim().to_ascii_lowercase();
                    cfg.native.capture_full_payloads =
                        matches!(normalized.as_str(), "1" | "true" | "yes" | "on");
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_AGENT_MODE") {
                    cfg.native.agent_mode =
                        Self::parse_native_agent_mode(raw, cfg.native.agent_mode);
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_MAX_TURNS") {
                    if let Ok(parsed) = raw.trim().parse::<u32>() {
                        cfg.native.max_turns = parsed.max(1);
                    }
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_TOKEN_BUDGET") {
                    if let Ok(parsed) = raw.trim().parse::<usize>() {
                        cfg.native.token_budget = parsed;
                    }
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_PROMPT_HEADROOM") {
                    if let Ok(parsed) = raw.trim().parse::<usize>() {
                        cfg.native.prompt_headroom = parsed;
                    }
                }
                if let Some(raw) = runtime.env.get("HIVEMIND_NATIVE_TIMEOUT_BUDGET_MS") {
                    if let Ok(parsed) = raw.trim().parse::<u64>() {
                        cfg.native.timeout_budget = Duration::from_millis(parsed);
                    }
                }
                let scripted_from_args = Self::parse_native_runtime_args(&mut cfg, &runtime_args);
                cfg.base.binary_path = PathBuf::from(runtime.binary_path);
                cfg.base.timeout = timeout;
                cfg.base.env = runtime.env;
                cfg.base.args = runtime_args;
                cfg.scripted_directives = if let Some(raw) =
                    cfg.base.env.get("HIVEMIND_NATIVE_SCRIPTED_DIRECTIVES_JSON")
                {
                    serde_json::from_str::<Vec<String>>(raw)
                        .unwrap_or_else(|_| cfg.scripted_directives.clone())
                } else if scripted_from_args.is_empty() {
                    cfg.scripted_directives.clone()
                } else {
                    scripted_from_args
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
    use crate::adapters::runtime::ExecutionInput;
    use tempfile::tempdir;

    #[test]
    fn native_runtime_adapter_respects_runtime_max_turns_and_timeout() {
        let state_dir = tempdir().unwrap();
        let worktree = tempdir().unwrap();
        let runtime = ProjectRuntimeConfig {
            adapter_name: "native".to_string(),
            binary_path: "builtin-native".to_string(),
            model: Some("mock-model".to_string()),
            args: vec![
                "--max-turns=24".to_string(),
                "--token-budget=64000".to_string(),
                "--prompt-headroom=1024".to_string(),
                "--agent-mode=freeflow".to_string(),
            ],
            env: HashMap::from([
                ("HIVEMIND_NATIVE_PROVIDER".to_string(), "mock".to_string()),
                (
                    "HIVEMIND_NATIVE_STATE_DIR".to_string(),
                    state_dir.path().display().to_string(),
                ),
                (
                    "HIVEMIND_NATIVE_SCRIPTED_DIRECTIVES_JSON".to_string(),
                    serde_json::to_string(&vec!["DONE:ok"]).unwrap(),
                ),
            ]),
            timeout_ms: 240_000,
            max_parallel_tasks: 1,
        };

        let mut adapter = match Registry::build_runtime_adapter(runtime).unwrap() {
            SelectedRuntimeAdapter::Native(adapter) => adapter,
            _ => panic!("expected native adapter"),
        };
        adapter.initialize().unwrap();
        adapter.prepare(Uuid::new_v4(), worktree.path()).unwrap();
        let report = adapter
            .execute(ExecutionInput {
                task_description: "Say ok".to_string(),
                success_criteria: "Return DONE".to_string(),
                context: None,
                prior_attempts: Vec::new(),
                verifier_feedback: None,
                native_prompt_metadata: None,
            })
            .unwrap();
        let trace = report.native_invocation.unwrap();

        assert_eq!(trace.configured_max_turns, Some(24));
        assert_eq!(trace.configured_timeout_budget_ms, Some(240_000));
        assert_eq!(trace.configured_token_budget, Some(64_000));
        assert_eq!(trace.configured_prompt_headroom, Some(1_024));
        assert_eq!(trace.agent_mode.as_deref(), Some("freeflow"));
    }
}
