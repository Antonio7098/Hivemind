use super::*;
use crate::native::turn_items::truncate_with_marker;
use crate::native::AgentMode;

impl Default for NativeToolEngine {
    fn default() -> Self {
        Self::new().unwrap_or_else(|error| {
            panic!(
                "native tool engine bootstrap failed ({}): {}",
                error.code, error.message
            )
        })
    }
}

struct EncodedTraceResponse {
    stored_response: String,
    prompt_response: String,
    original_bytes: Option<usize>,
    stored_bytes: Option<usize>,
    truncated: bool,
}

impl NativeToolEngine {
    fn encode_traced_response(output: &Value) -> EncodedTraceResponse {
        let response_payload = json!({
            "ok": true,
            "output": output,
        });
        let prompt_response = serde_json::to_string(&response_payload)
            .unwrap_or_else(|error| format!("{{\"ok\":false,\"encode_error\":\"{error}\"}}"));
        let original_bytes = prompt_response.len();
        if prompt_response.chars().count() <= TOOL_TRACE_RESPONSE_MAX_CHARS {
            return EncodedTraceResponse {
                stored_response: prompt_response.clone(),
                prompt_response,
                original_bytes: Some(original_bytes),
                stored_bytes: Some(original_bytes),
                truncated: false,
            };
        }

        let preview = truncate_with_marker(
            &prompt_response,
            TOOL_TRACE_RESPONSE_MAX_CHARS.saturating_sub(256),
        );
        let truncated_payload = json!({
            "ok": true,
            "output_truncated": true,
            "original_bytes": original_bytes,
            "stored_bytes": preview.len(),
            "preview": preview,
        });
        let stored_response = serde_json::to_string(&truncated_payload)
            .unwrap_or_else(|error| format!("{{\"ok\":false,\"encode_error\":\"{error}\"}}"));
        EncodedTraceResponse {
            stored_bytes: Some(stored_response.len()),
            stored_response,
            prompt_response,
            original_bytes: Some(original_bytes),
            truncated: true,
        }
    }

    #[must_use]
    pub fn contracts(&self) -> Vec<ToolContract> {
        self.tools
            .values()
            .map(|tool| tool.contract.clone())
            .collect()
    }

    #[must_use]
    pub fn contracts_for_mode(&self, mode: AgentMode) -> Vec<ToolContract> {
        self.tools
            .values()
            .filter(|tool| mode.allows_permissions(&tool.contract.required_permissions))
            .map(|tool| tool.contract.clone())
            .collect()
    }

    pub fn new() -> Result<Self, NativeToolEngineError> {
        let mut engine = Self {
            tools: BTreeMap::new(),
        };
        engine.register_builtin::<ReadFileInput, ReadFileOutput>(
            "read_file",
            "filesystem_read",
            vec![ToolPermission::FilesystemRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_read_file,
        )?;
        engine.register_builtin::<ListFilesInput, ListFilesOutput>(
            "list_files",
            "filesystem_read",
            vec![ToolPermission::FilesystemRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_list_files,
        )?;
        engine.register_builtin::<WriteFileInput, WriteFileOutput>(
            "write_file",
            "filesystem_write",
            vec![ToolPermission::FilesystemWrite],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_write_file,
        )?;
        engine.register_builtin::<RunCommandInput, RunCommandOutput>(
            "run_command",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            false,
            handle_run_command,
        )?;
        engine.register_builtin::<CheckpointCompleteInput, CheckpointCompleteOutput>(
            "checkpoint_complete",
            "orchestration_checkpoint",
            vec![ToolPermission::Execution],
            CHECKPOINT_COMPLETE_TIMEOUT_MS,
            false,
            handle_checkpoint_complete,
        )?;
        engine.register_builtin::<RetainContextInput, RetainContextOutput>(
            "retain_context",
            "context_management",
            vec![],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_retain_context,
        )?;
        engine.register_builtin::<ExecCommandInput, ExecSessionOutput>(
            "exec_command",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_exec_command,
        )?;
        engine.register_builtin::<WriteStdinInput, ExecSessionOutput>(
            "write_stdin",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_write_stdin,
        )?;
        engine.register_builtin::<NoInput, GitStatusOutput>(
            "git_status",
            "repository_read",
            vec![ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_git_status,
        )?;
        engine.register_builtin::<GitDiffInput, GitDiffOutput>(
            "git_diff",
            "repository_read",
            vec![ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_git_diff,
        )?;
        engine.register_builtin::<GraphQueryInput, GraphQueryResult>(
            "graph_query",
            "graph_query_read",
            vec![ToolPermission::FilesystemRead, ToolPermission::GitRead],
            GRAPH_QUERY_TIMEOUT_MS,
            true,
            handle_graph_query,
        )?;
        Ok(engine)
    }

    fn register_builtin<I, O>(
        &mut self,
        name: &str,
        required_scope: &str,
        required_permissions: Vec<ToolPermission>,
        timeout_ms: u64,
        cancellable: bool,
        handler: ToolHandler,
    ) -> Result<(), NativeToolEngineError>
    where
        I: JsonSchema,
        O: JsonSchema,
    {
        let input_schema = serde_json::to_value(schema_for!(I)).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to encode input schema for '{name}': {error}"
            ))
        })?;
        let output_schema = serde_json::to_value(schema_for!(O)).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to encode output schema for '{name}': {error}"
            ))
        })?;
        let input_validator = JSONSchema::compile(&input_schema).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to compile input schema for '{name}': {error}"
            ))
        })?;
        let output_validator = JSONSchema::compile(&output_schema).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to compile output schema for '{name}': {error}"
            ))
        })?;

        let contract = ToolContract {
            name: name.to_string(),
            version: TOOL_VERSION_V1.to_string(),
            required_scope: required_scope.to_string(),
            required_permissions,
            timeout_ms,
            cancellable,
            input_schema,
            output_schema,
        };

        self.tools.insert(
            Self::tool_key(name, TOOL_VERSION_V1),
            RegisteredTool {
                contract,
                input_validator,
                output_validator,
                handler,
            },
        );
        Ok(())
    }

    fn tool_key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    fn validate_input(
        validator: &JSONSchema,
        tool_name: &str,
        payload: &Value,
    ) -> Result<(), NativeToolEngineError> {
        validator.validate(payload).map_err(|iter| {
            let first = iter.into_iter().next();
            let message = first.map_or_else(
                || "schema validation failed".to_string(),
                |error| format!("path '{}' violated schema: {}", error.instance_path, error),
            );
            NativeToolEngineError::validation(format!(
                "tool '{tool_name}' input is invalid: {message}"
            ))
        })
    }

    fn normalize_action_input(action: &NativeToolAction, worktree: &Path) -> Value {
        let mut payload = Self::unwrap_input_wrapper(&action.input);
        match action.name.as_str() {
            "run_command" => {
                Self::normalize_command_input(&mut payload, "command", &["cmd"]);
                if let Some(object) = payload.as_object_mut() {
                    object.remove("cwd");
                }
            }
            "exec_command" => {
                Self::normalize_command_input(&mut payload, "cmd", &["command"]);
                Self::normalize_path_field(&mut payload, "cwd", worktree, true);
            }
            "read_file" | "write_file" | "retain_context" => {
                Self::normalize_path_field(&mut payload, "path", worktree, false);
            }
            "list_files" => {
                Self::normalize_path_field(&mut payload, "path", worktree, true);
            }
            _ => {}
        }
        payload
    }

    fn unwrap_input_wrapper(payload: &Value) -> Value {
        let Some(object) = payload.as_object() else {
            return payload.clone();
        };
        if let Some(arguments) = object.get("arguments").filter(|value| value.is_object()) {
            return arguments.clone();
        }
        if (object.contains_key("action")
            || object.contains_key("tool")
            || object.contains_key("name"))
            && object.get("input").is_some_and(Value::is_object)
        {
            return object
                .get("input")
                .cloned()
                .unwrap_or_else(|| payload.clone());
        }
        payload.clone()
    }

    fn normalize_command_input(payload: &mut Value, primary: &str, aliases: &[&str]) {
        let Some(object) = payload.as_object_mut() else {
            return;
        };
        if !object.contains_key(primary) {
            for alias in aliases {
                if let Some(value) = object.remove(*alias) {
                    object.insert(primary.to_string(), value);
                    break;
                }
            }
        }
        let has_args = object
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|args| !args.is_empty());
        if has_args {
            return;
        }
        let Some(raw_command) = object.get(primary).and_then(Value::as_str) else {
            return;
        };
        let Some((command, args)) = split_command_string(raw_command) else {
            return;
        };
        object.insert(primary.to_string(), Value::String(command));
        object.insert(
            "args".to_string(),
            Value::Array(args.into_iter().map(Value::String).collect()),
        );
    }

    fn normalize_path_field(payload: &mut Value, key: &str, worktree: &Path, allow_empty: bool) {
        let Some(object) = payload.as_object_mut() else {
            return;
        };
        let Some(raw) = object.get(key).and_then(Value::as_str) else {
            return;
        };
        let Some(normalized) = Self::normalize_worktree_relative_path(raw, worktree, allow_empty)
        else {
            return;
        };
        object.insert(key.to_string(), Value::String(normalized));
    }

    fn normalize_worktree_relative_path(
        raw: &str,
        worktree: &Path,
        allow_empty: bool,
    ) -> Option<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        let candidate = Path::new(trimmed);
        let raw_relative = if candidate.is_absolute() {
            candidate.strip_prefix(worktree).ok()?.to_path_buf()
        } else {
            PathBuf::from(trimmed)
        };
        normalize_relative_path(&raw_relative.to_string_lossy(), allow_empty)
            .ok()
            .map(|path| relative_display(&path))
    }

    fn validate_output(
        validator: &JSONSchema,
        tool_name: &str,
        payload: &Value,
    ) -> Result<(), NativeToolEngineError> {
        validator.validate(payload).map_err(|iter| {
            let first = iter.into_iter().next();
            let message = first.map_or_else(
                || "schema validation failed".to_string(),
                |error| format!("path '{}' violated schema: {}", error.instance_path, error),
            );
            NativeToolEngineError::output_validation(tool_name, message)
        })
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_tool_policies(
        action: &NativeToolAction,
        tool: &RegisteredTool,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<Vec<String>, NativeToolEngineError> {
        evaluate_tool_policies_impl(action, tool, ctx)
    }

    fn execute_internal(
        &self,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<(Value, Vec<String>), NativeToolEngineError> {
        let tool_key = Self::tool_key(&action.name, &action.version);
        let Some(tool) = self.tools.get(&tool_key) else {
            return Err(NativeToolEngineError::unknown_tool(&action.name));
        };
        let normalized_action = NativeToolAction {
            input: Self::normalize_action_input(action, ctx.worktree),
            ..action.clone()
        };

        Self::validate_input(
            &tool.input_validator,
            &action.name,
            &normalized_action.input,
        )?;
        let policy_tags = Self::evaluate_tool_policies(&normalized_action, tool, ctx)?;
        let started = Instant::now();
        let output = (tool.handler)(ctx, &normalized_action.input, tool.contract.timeout_ms)?;
        if started.elapsed() > Duration::from_millis(tool.contract.timeout_ms) {
            return Err(NativeToolEngineError::timeout(
                &action.name,
                tool.contract.timeout_ms,
            ));
        }
        Self::validate_output(&tool.output_validator, &action.name, &output)?;
        Ok((output, policy_tags))
    }

    pub fn execute(
        &self,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<Value, NativeToolEngineError> {
        self.execute_internal(action, ctx).map(|(output, _)| output)
    }

    pub fn execute_action_trace(
        &self,
        call_id: String,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> NativeToolCallTrace {
        let started = Instant::now();
        let request_payload = json!({
            "tool": action.name,
            "version": action.version,
            "input": action.input
        });
        let request = serde_json::to_string(&request_payload).unwrap_or_else(|error| {
            format!(
                "{{\"tool\":\"{}\",\"encode_error\":\"{}\"}}",
                action.name, error
            )
        });

        match self.execute_internal(action, ctx) {
            Ok((output, policy_tags)) => {
                let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let encoded_response = Self::encode_traced_response(&output);
                NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    duration_ms: Some(duration_ms),
                    response: Some(encoded_response.stored_response),
                    prompt_response: Some(encoded_response.prompt_response),
                    response_original_bytes: encoded_response.original_bytes,
                    response_stored_bytes: encoded_response.stored_bytes,
                    response_truncated: encoded_response.truncated,
                    failure: None,
                    policy_tags,
                }
            }
            Err(error) => {
                let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let recovery_hint = recovery_hint_for_tool_error(action, &error);
                NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    duration_ms: Some(duration_ms),
                    response: None,
                    prompt_response: None,
                    response_original_bytes: None,
                    response_stored_bytes: None,
                    response_truncated: false,
                    failure: Some(NativeToolCallFailure {
                        code: error.code.clone(),
                        message: error.message.clone(),
                        recoverable: error.recoverable,
                        policy_source: None,
                        denial_reason: None,
                        recovery_hint,
                    }),
                    policy_tags: error.policy_tags,
                }
            }
        }
    }

    pub fn execute_action_trace_for_mode(
        &self,
        mode: AgentMode,
        call_id: String,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> NativeToolCallTrace {
        let tool_key = Self::tool_key(&action.name, &action.version);
        let request_payload = json!({
            "tool": action.name,
            "version": action.version,
            "input": action.input
        });
        let request = serde_json::to_string(&request_payload).unwrap_or_else(|error| {
            format!(
                "{{\"tool\":\"{}\",\"encode_error\":\"{}\"}}",
                action.name, error
            )
        });

        if let Some(tool) = self.tools.get(&tool_key) {
            if !mode.allows_permissions(&tool.contract.required_permissions) {
                return NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    duration_ms: Some(0),
                    response: None,
                    prompt_response: None,
                    response_original_bytes: None,
                    response_stored_bytes: None,
                    response_truncated: false,
                    failure: Some(NativeToolCallFailure {
                        code: "native_tool_mode_denied".to_string(),
                        message: format!(
                            "tool '{}' is not permitted in agent mode '{}'",
                            action.name,
                            mode.as_str()
                        ),
                        recoverable: true,
                        policy_source: Some("agent_mode_policy".to_string()),
                        denial_reason: Some(format!(
                            "agent mode '{}' does not allow tool '{}'",
                            mode.as_str(),
                            action.name
                        )),
                        recovery_hint: Some(
                            "Use an allowed read-only tool for planner mode, or switch to task_executor/freeflow for mutations."
                                .to_string(),
                        ),
                    }),
                    policy_tags: vec![format!("agent_mode:{}", mode.as_str())],
                };
            }
        }

        self.execute_action_trace(call_id, action, ctx)
    }
}

fn recovery_hint_for_tool_error(
    action: &NativeToolAction,
    error: &NativeToolEngineError,
) -> Option<String> {
    let message = error.message.to_ascii_lowercase();
    match action.name.as_str() {
        "read_file" | "list_files" | "write_file" => {
            if error.code == "native_tool_input_invalid" && message.contains("relative path") {
                return Some(
                    "Use repository-relative paths like `src/native/turn_items.rs`, not absolute sandbox or worktree paths."
                        .to_string(),
                );
            }
            if error.code == "native_tool_execution_failed"
                && (message.contains("no such file")
                    || message.contains("not found")
                    || message.contains("not a directory")
                    || message.contains("is a directory"))
            {
                return Some(
                    "Check the repository-relative path first. If the target might be a directory, use `list_files`; if it should be a file, use `read_file`."
                        .to_string(),
                );
            }
        }
        "run_command" | "exec_command" => {
            if error.code == "native_policy_violation" && message.contains("grep") {
                return Some(
                    "Use `rg` instead of `grep`, and prefer repository-relative paths under `src/`."
                        .to_string(),
                );
            }
        }
        "graph_query" => {
            if error.code == "native_tool_execution_failed" && message.contains("broken pipe") {
                return Some(
                    "If `graph_query` fails, fall back to `list_files`, `read_file`, or `run_command` with allowlisted tools like `rg`."
                        .to_string(),
                );
            }
        }
        _ => {}
    }
    None
}
