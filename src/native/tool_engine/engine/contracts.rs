use super::*;
use crate::native::AgentMode;

impl NativeToolEngine {
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

    pub(super) fn tool_key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    pub(super) fn validate_input(
        validator: &JSONSchema,
        tool_name: &str,
        payload: &Value,
    ) -> Result<(), NativeToolEngineError> {
        let normalized = Self::normalize_wrapped_tool_payload(tool_name, payload);
        validator.validate(&normalized).map_err(|iter| {
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

    pub(super) fn normalize_wrapped_tool_payload(tool_name: &str, payload: &Value) -> Value {
        let Some(object) = payload.as_object() else {
            return payload.clone();
        };
        let mut normalized = if let Some(arguments) = object.get("arguments") {
            let action_name_matches = object
                .get("action")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(tool_name));
            if action_name_matches {
                arguments.clone()
            } else {
                payload.clone()
            }
        } else {
            payload.clone()
        };

        let Some(map) = normalized.as_object_mut() else {
            return normalized;
        };

        match tool_name {
            "run_command" => {
                if let Some(cmd) = map.remove("cmd") {
                    map.entry("command".to_string()).or_insert(cmd);
                }
                map.remove("cwd");
            }
            "exec_command" => {
                if let Some(command) = map.remove("command") {
                    map.entry("cmd".to_string()).or_insert(command);
                }
            }
            _ => {}
        }

        normalized
    }

    pub(super) fn validate_output(
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
}
