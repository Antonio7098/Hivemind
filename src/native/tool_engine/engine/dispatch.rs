use super::*;
use crate::native::AgentMode;

impl NativeToolEngine {
    // ARCH_DEBT: legacy oversized function
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
        let normalized_input = Self::normalize_wrapped_tool_payload(&action.name, &action.input);
        Self::validate_input(&tool.input_validator, &action.name, &normalized_input)?;
        let normalized_action = NativeToolAction {
            name: action.name.clone(),
            version: action.version.clone(),
            input: normalized_input.clone(),
        };
        let policy_tags = Self::evaluate_tool_policies(&normalized_action, tool, ctx)?;
        let started = Instant::now();
        let output = (tool.handler)(ctx, &normalized_input, tool.contract.timeout_ms)?;
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
        let request = encode_action_request(action);
        match self.execute_internal(action, ctx) {
            Ok((output, policy_tags)) => {
                let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let response_payload = json!({"ok": true, "output": output});
                let mut response =
                    serde_json::to_string(&response_payload).unwrap_or_else(|error| {
                        format!("{{\"ok\":false,\"encode_error\":\"{error}\"}}")
                    });
                let response_original_bytes = Some(response.len());
                let mut response_truncated = false;
                if response.len() > TOOL_TRACE_RESPONSE_MAX_CHARS {
                    response_truncated = true;
                    response = json!({
                        "ok": true,
                        "output_truncated": true,
                        "stored_chars": TOOL_TRACE_RESPONSE_MAX_CHARS,
                    })
                    .to_string();
                }
                let response_stored_bytes = Some(response.len());
                NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    duration_ms: Some(duration_ms),
                    response: Some(response),
                    prompt_response: None,
                    response_original_bytes,
                    response_stored_bytes,
                    response_truncated,
                    failure: None,
                    policy_tags,
                }
            }
            Err(error) => {
                let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
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
                        recovery_hint: Self::recovery_hint_for_error(action, &error),
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
        let request = encode_action_request(action);
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
                        message: format!("tool '{}' is not permitted in agent mode '{}'", action.name, mode.as_str()),
                        recoverable: true,
                        policy_source: Some("agent_mode_policy".to_string()),
                        denial_reason: Some(format!("agent mode '{}' does not allow tool '{}'", mode.as_str(), action.name)),
                        recovery_hint: Some("Use an allowed read-only tool for planner mode, or switch to task_executor/freeflow for mutations.".to_string()),
                    }),
                    policy_tags: vec![format!("agent_mode:{}", mode.as_str())],
                };
            }
        }
        self.execute_action_trace(call_id, action, ctx)
    }

    fn recovery_hint_for_error(
        action: &NativeToolAction,
        error: &NativeToolEngineError,
    ) -> Option<String> {
        if error.code == "native_policy_violation"
            && action.name == "run_command"
            && action
                .input
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| command.eq_ignore_ascii_case("grep"))
        {
            return Some("Use `rg` instead of `grep` for repository searches.".to_string());
        }

        if error.code == "native_tool_input_invalid"
            && matches!(
                action.name.as_str(),
                "list_files" | "read_file" | "write_file"
            )
        {
            let hint = if action.name == "list_files" {
                "Use repository-relative paths instead of absolute paths."
            } else {
                "Use repository-relative paths when invoking filesystem tools."
            };
            if error.message.contains("invalid relative path")
                || error.message.contains("path cannot be empty")
            {
                return Some(hint.to_string());
            }
        }

        if error.code == "native_tool_execution_failed"
            && action.name == "read_file"
            && error.recoverable
        {
            return Some(
                "Check the repository-relative path first, then confirm the file exists before retrying."
                    .to_string(),
            );
        }

        None
    }
}

fn encode_action_request(action: &NativeToolAction) -> String {
    let request_payload =
        json!({"tool": action.name, "version": action.version, "input": action.input});
    serde_json::to_string(&request_payload).unwrap_or_else(|error| {
        format!(
            "{{\"tool\":\"{}\",\"encode_error\":\"{}\"}}",
            action.name, error
        )
    })
}
