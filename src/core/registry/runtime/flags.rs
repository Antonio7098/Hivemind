use super::*;

impl Registry {
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
                let mut flags = vec![
                    format!("--max-turns={}", NativeRuntimeConfig::default().max_turns),
                    format!(
                        "--token-budget={}",
                        NativeRuntimeConfig::default().token_budget
                    ),
                ];
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
