use super::*;

impl Registry {
    fn has_long_flag(args: &[String], name: &str) -> bool {
        let flag = format!("--{name}");
        let prefix = format!("{flag}=");
        args.iter().enumerate().any(|(index, arg)| {
            arg == &flag || arg.starts_with(&prefix) || index > 0 && args[index - 1] == flag
        })
    }

    pub(crate) fn runtime_env_build_error(
        error: RuntimeEnvBuildError,
        origin: &'static str,
    ) -> HivemindError {
        HivemindError::runtime(error.code, error.message, origin).recoverable(false)
    }

    pub(crate) fn runtime_start_flags(runtime: &ProjectRuntimeConfig) -> Vec<String> {
        match runtime.adapter_name.as_str() {
            "codex" => {
                let mut flags = if runtime.args.is_empty() {
                    CodexConfig::default().base.args
                } else {
                    runtime.args.clone()
                };
                if let Some(model) = runtime.model.as_ref() {
                    if !Self::has_model_flag(&flags, false) {
                        flags.extend(["--model".to_string(), model.clone()]);
                    }
                }
                flags
            }
            "claude-code" => {
                let mut flags = if runtime.args.is_empty() {
                    ClaudeCodeConfig::default().base.args
                } else {
                    runtime.args.clone()
                };
                if let Some(model) = runtime.model.as_ref() {
                    if !Self::has_model_flag(&flags, false) {
                        flags.extend(["--model".to_string(), model.clone()]);
                    }
                }
                flags
            }
            "opencode" | "kilo" => {
                let mut flags = runtime.args.clone();
                let is_opencode_binary = PathBuf::from(&runtime.binary_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| {
                        let lower = s.to_ascii_lowercase();
                        lower.contains("opencode") || lower.contains("kilo")
                    });

                if is_opencode_binary {
                    let mut with_start = vec!["run".to_string()];
                    if let Some(model) = runtime.model.as_ref() {
                        if !Self::has_model_flag(&flags, true) {
                            with_start.extend(["--model".to_string(), model.clone()]);
                        }
                    }
                    with_start.append(&mut flags);
                    with_start
                } else {
                    flags
                }
            }
            "native" => {
                let defaults = NativeRuntimeConfig::default();
                let max_turns = runtime
                    .env
                    .get("HIVEMIND_NATIVE_MAX_TURNS")
                    .and_then(|raw| raw.trim().parse::<u32>().ok())
                    .filter(|value| *value > 0)
                    .unwrap_or(defaults.max_turns);
                let agent_mode = runtime
                    .env
                    .get("HIVEMIND_NATIVE_AGENT_MODE")
                    .map(|raw| raw.trim().to_ascii_lowercase())
                    .and_then(|raw| match raw.as_str() {
                        "planner" => Some(crate::native::AgentMode::Planner),
                        "freeflow" => Some(crate::native::AgentMode::Freeflow),
                        "task_executor" | "task-executor" | "executor" => {
                            Some(crate::native::AgentMode::TaskExecutor)
                        }
                        _ => None,
                    })
                    .unwrap_or(defaults.agent_mode);
                let token_budget = runtime
                    .env
                    .get("HIVEMIND_NATIVE_TOKEN_BUDGET")
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(defaults.token_budget);
                let prompt_headroom = runtime
                    .env
                    .get("HIVEMIND_NATIVE_PROMPT_HEADROOM")
                    .and_then(|raw| raw.trim().parse::<usize>().ok())
                    .unwrap_or(defaults.prompt_headroom);
                let mut flags = Vec::new();
                if !Self::has_long_flag(&runtime.args, "max-turns") {
                    flags.push(format!("--max-turns={max_turns}"));
                }
                if !Self::has_long_flag(&runtime.args, "token-budget") {
                    flags.push(format!("--token-budget={token_budget}"));
                }
                if !Self::has_long_flag(&runtime.args, "prompt-headroom") {
                    flags.push(format!("--prompt-headroom={prompt_headroom}"));
                }
                if !Self::has_long_flag(&runtime.args, "agent-mode") {
                    flags.push(format!("--agent-mode={}", agent_mode.as_str()));
                }
                flags.extend(runtime.args.clone());
                flags
            }
            _ => runtime.args.clone(),
        }
    }

    pub(crate) fn runtime_descriptor_for_adapter(
        adapter: &str,
    ) -> Option<crate::adapters::RuntimeDescriptor> {
        runtime_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.adapter_name == adapter)
    }

    pub(crate) fn runtime_capabilities_for_adapter(adapter: &str) -> Vec<String> {
        Self::runtime_descriptor_for_adapter(adapter)
            .map(|descriptor| {
                descriptor
                    .capabilities
                    .iter()
                    .map(|capability| (*capability).to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_runtime_config() -> ProjectRuntimeConfig {
        ProjectRuntimeConfig {
            adapter_name: "native".to_string(),
            binary_path: "builtin-native".to_string(),
            model: Some("demo-model".to_string()),
            args: Vec::new(),
            env: HashMap::new(),
            timeout_ms: 60_000,
            max_parallel_tasks: 1,
        }
    }

    #[test]
    fn native_runtime_flags_apply_env_max_turn_override() {
        let mut runtime = native_runtime_config();
        runtime
            .env
            .insert("HIVEMIND_NATIVE_MAX_TURNS".to_string(), "24".to_string());

        let flags = Registry::runtime_start_flags(&runtime);

        assert!(flags.iter().any(|flag| flag == "--max-turns=24"));
    }

    #[test]
    fn native_runtime_flags_preserve_explicit_arg_overrides() {
        let mut runtime = native_runtime_config();
        runtime.args = vec![
            "--max-turns=31".to_string(),
            "--token-budget=64000".to_string(),
            "--prompt-headroom=1024".to_string(),
            "--agent-mode=task_executor".to_string(),
        ];

        let flags = Registry::runtime_start_flags(&runtime);

        assert_eq!(
            flags
                .iter()
                .filter(|flag| flag.starts_with("--max-turns"))
                .count(),
            1
        );
        assert!(flags.iter().any(|flag| flag == "--max-turns=31"));
        assert!(flags.iter().any(|flag| flag == "--token-budget=64000"));
        assert!(flags.iter().any(|flag| flag == "--prompt-headroom=1024"));
        assert!(flags
            .iter()
            .any(|flag| flag == "--agent-mode=task_executor"));
    }

    #[test]
    fn native_runtime_flags_preserve_split_form_explicit_overrides() {
        let mut runtime = native_runtime_config();
        runtime.args = vec![
            "--max-turns".to_string(),
            "31".to_string(),
            "--token-budget".to_string(),
            "64000".to_string(),
            "--prompt-headroom".to_string(),
            "1024".to_string(),
            "--agent-mode".to_string(),
            "task_executor".to_string(),
        ];

        let flags = Registry::runtime_start_flags(&runtime);

        assert_eq!(
            flags
                .iter()
                .filter(|flag| flag.starts_with("--max-turns"))
                .count(),
            1
        );
        assert_eq!(
            flags
                .iter()
                .filter(|flag| flag.starts_with("--token-budget"))
                .count(),
            1
        );
        assert_eq!(
            flags
                .iter()
                .filter(|flag| flag.starts_with("--prompt-headroom"))
                .count(),
            1
        );
        assert_eq!(
            flags
                .iter()
                .filter(|flag| flag.starts_with("--agent-mode"))
                .count(),
            1
        );
        assert!(flags
            .windows(2)
            .any(|pair| pair[0] == "--max-turns" && pair[1] == "31"));
    }
}
