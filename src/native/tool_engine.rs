//! Native typed tool engine with schema validation and policy-aware dispatch.
//!
//! Sprint 44 introduces a deterministic tool registry for native runtime turns.

use crate::adapters::runtime::{NativeToolCallFailure, NativeToolCallTrace};
use crate::core::scope::Scope;
use jsonschema::JSONSchema;
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const TOOL_VERSION_V1: &str = "1.0.0";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// Structured native tool engine error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolEngineError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl NativeToolEngineError {
    fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }

    fn unknown_tool(tool_name: &str) -> Self {
        Self::new(
            "native_tool_unknown",
            format!("Unknown native tool '{tool_name}'"),
            false,
        )
    }

    fn validation(error: impl Into<String>) -> Self {
        Self::new("native_tool_input_invalid", error, false)
    }

    fn output_validation(tool_name: &str, error: impl Into<String>) -> Self {
        Self::new(
            "native_tool_output_invalid",
            format!(
                "Tool '{tool_name}' produced invalid output: {}",
                error.into()
            ),
            false,
        )
    }

    fn scope_violation(error: impl Into<String>) -> Self {
        Self::new("native_scope_violation", error, false)
    }

    fn policy_violation(error: impl Into<String>) -> Self {
        Self::new("native_policy_violation", error, false)
    }

    fn execution(error: impl Into<String>) -> Self {
        Self::new("native_tool_execution_failed", error, false)
    }

    fn timeout(tool_name: &str, timeout_ms: u64) -> Self {
        Self::new(
            "native_tool_timeout",
            format!("Tool '{tool_name}' exceeded timeout envelope ({timeout_ms}ms)"),
            false,
        )
    }
}

/// High-level capability requirement for a tool contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    FilesystemRead,
    FilesystemWrite,
    Execution,
    GitRead,
}

/// Declared tool contract used by the deterministic registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolContract {
    pub name: String,
    pub version: String,
    pub required_scope: String,
    pub required_permissions: Vec<ToolPermission>,
    pub timeout_ms: u64,
    pub cancellable: bool,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// Parsed native tool action from `ACT:` directives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolAction {
    pub name: String,
    #[serde(default = "default_tool_version")]
    pub version: String,
    #[serde(default)]
    pub input: Value,
}

fn default_tool_version() -> String {
    TOOL_VERSION_V1.to_string()
}

impl NativeToolAction {
    /// Parses `tool:` directives in one of these forms:
    /// - `tool:<name>:<json-object>`
    /// - `tool:{"name":"...","version":"...","input":{...}}`
    pub fn parse(action: &str) -> Result<Option<Self>, NativeToolEngineError> {
        let Some(raw) = action.trim().strip_prefix("tool:") else {
            return Ok(None);
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool action is missing tool name/payload",
            ));
        }

        if raw.starts_with('{') {
            return serde_json::from_str::<Self>(raw)
                .map(Some)
                .map_err(|error| {
                    NativeToolEngineError::validation(format!(
                        "tool action JSON payload is invalid: {error}"
                    ))
                });
        }

        let mut parts = raw.splitn(2, ':');
        let tool_name = parts.next().unwrap_or_default().trim();
        if tool_name.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool name cannot be empty",
            ));
        }

        let input = match parts.next().map(str::trim) {
            None | Some("") => json!({}),
            Some(payload) => serde_json::from_str::<Value>(payload).map_err(|error| {
                NativeToolEngineError::validation(format!(
                    "tool input JSON payload is invalid: {error}"
                ))
            })?,
        };

        Ok(Some(Self {
            name: tool_name.to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input,
        }))
    }
}

/// Command policy for `run_command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCommandPolicy {
    pub allowlist: Vec<String>,
    pub denylist: Vec<String>,
    pub deny_by_default: bool,
}

impl Default for NativeCommandPolicy {
    fn default() -> Self {
        Self {
            allowlist: Vec::new(),
            denylist: vec!["rm".to_string(), "sudo".to_string()],
            deny_by_default: true,
        }
    }
}

impl NativeCommandPolicy {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut policy = Self::default();
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST") {
            policy.allowlist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENYLIST") {
            policy.denylist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENY_BY_DEFAULT") {
            policy.deny_by_default = parse_bool_with_default(raw, true);
        }
        policy
    }

    #[must_use]
    pub fn is_allowed(&self, command_line: &str) -> bool {
        if self
            .denylist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return false;
        }
        if self
            .allowlist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return true;
        }
        !self.deny_by_default
    }
}

/// Execution context passed to tool handlers.
pub struct ToolExecutionContext<'a> {
    pub worktree: &'a Path,
    pub scope: Option<&'a Scope>,
    pub command_policy: &'a NativeCommandPolicy,
}

type ToolHandler =
    fn(&ToolExecutionContext<'_>, &Value, u64) -> Result<Value, NativeToolEngineError>;

struct RegisteredTool {
    contract: ToolContract,
    input_validator: JSONSchema,
    output_validator: JSONSchema,
    handler: ToolHandler,
}

/// Deterministic, typed native tool registry and dispatcher.
pub struct NativeToolEngine {
    tools: BTreeMap<String, RegisteredTool>,
}

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

impl NativeToolEngine {
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

    pub fn execute(
        &self,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<Value, NativeToolEngineError> {
        let tool_key = Self::tool_key(&action.name, &action.version);
        let Some(tool) = self.tools.get(&tool_key) else {
            return Err(NativeToolEngineError::unknown_tool(&action.name));
        };

        Self::validate_input(&tool.input_validator, &action.name, &action.input)?;
        let started = Instant::now();
        let output = (tool.handler)(ctx, &action.input, tool.contract.timeout_ms)?;
        if started.elapsed() > Duration::from_millis(tool.contract.timeout_ms) {
            return Err(NativeToolEngineError::timeout(
                &action.name,
                tool.contract.timeout_ms,
            ));
        }
        Self::validate_output(&tool.output_validator, &action.name, &output)?;
        Ok(output)
    }

    pub fn execute_action_trace(
        &self,
        call_id: String,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> NativeToolCallTrace {
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

        match self.execute(action, ctx) {
            Ok(output) => {
                let response_payload = json!({
                    "ok": true,
                    "output": output,
                });
                NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    response: Some(serde_json::to_string(&response_payload).unwrap_or_else(
                        |error| format!("{{\"ok\":false,\"encode_error\":\"{error}\"}}"),
                    )),
                    failure: None,
                }
            }
            Err(error) => NativeToolCallTrace {
                call_id,
                tool_name: action.name.clone(),
                request,
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: error.code,
                    message: error.message,
                    recoverable: error.recoverable,
                }),
            },
        }
    }
}

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_bool_with_default(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn matches_command_pattern(pattern: &str, command_line: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    pattern == "*"
        || command_line == pattern
        || command_line.starts_with(&format!("{pattern} "))
        || command_line.starts_with(&format!("{pattern}/"))
}

fn normalize_relative_path(raw: &str, allow_empty: bool) -> Result<PathBuf, NativeToolEngineError> {
    let mut normalized = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(NativeToolEngineError::validation(format!(
                    "invalid relative path '{raw}'"
                )));
            }
        }
    }

    if normalized.as_os_str().is_empty() && !allow_empty {
        return Err(NativeToolEngineError::validation(
            "path cannot be empty or current-directory only",
        ));
    }

    Ok(normalized)
}

fn relative_display(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".to_string()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn decode_input<T: DeserializeOwned>(input: &Value) -> Result<T, NativeToolEngineError> {
    let raw = input.to_string();
    let mut deserializer = serde_json::Deserializer::from_str(&raw);
    serde_path_to_error::deserialize::<_, T>(&mut deserializer).map_err(|error| {
        NativeToolEngineError::validation(format!(
            "typed decode failed at '{}': {}",
            error.path(),
            error
        ))
    })
}

fn ensure_can_read(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_read(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "read is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

fn ensure_can_write(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_write(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "write is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

fn ensure_repository_read(scope: Option<&Scope>) -> Result<(), NativeToolEngineError> {
    let Some(scope) = scope else {
        return Ok(());
    };
    if scope.repositories.is_empty() {
        return Ok(());
    }
    let readable = scope.repositories.iter().any(|repo| {
        matches!(
            repo.mode,
            crate::core::scope::RepoAccessMode::ReadOnly
                | crate::core::scope::RepoAccessMode::ReadWrite
        )
    });
    if readable {
        Ok(())
    } else {
        Err(NativeToolEngineError::scope_violation(
            "repository read access is not permitted by scope",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadFileInput {
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadFileOutput {
    path: String,
    content: String,
    byte_size: u64,
}

fn handle_read_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ReadFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_read(ctx.scope, &rel)?;
    let path = ctx.worktree.join(&rel);
    let content = fs::read_to_string(&path).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to read '{}': {error}", path.display()))
    })?;
    let output = ReadFileOutput {
        path: relative_display(&rel),
        byte_size: u64::try_from(content.len()).unwrap_or(u64::MAX),
        content,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode read_file output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    include_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesEntry {
    path: String,
    is_dir: bool,
    byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesOutput {
    base_path: String,
    entries: Vec<ListFilesEntry>,
}

fn handle_list_files(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ListFilesInput>(input)?;
    let rel = normalize_relative_path(
        input.path.as_deref().unwrap_or("."),
        true, // allow listing current directory.
    )?;

    if !rel.as_os_str().is_empty() {
        ensure_can_read(ctx.scope, &rel)?;
    }
    let root = ctx.worktree.join(&rel);
    let mut entries = Vec::new();
    list_entries(
        ctx,
        &root,
        &rel,
        input.recursive,
        input.include_hidden,
        &mut entries,
    )?;

    let output = ListFilesOutput {
        base_path: relative_display(&rel),
        entries,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode list_files output: {error}"))
    })
}

fn list_entries(
    ctx: &ToolExecutionContext<'_>,
    absolute_dir: &Path,
    relative_dir: &Path,
    recursive: bool,
    include_hidden: bool,
    entries: &mut Vec<ListFilesEntry>,
) -> Result<(), NativeToolEngineError> {
    let read_dir = fs::read_dir(absolute_dir).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to read directory '{}': {error}",
            absolute_dir.display()
        ))
    })?;

    for item in read_dir {
        let item = item.map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to inspect directory entry in '{}': {error}",
                absolute_dir.display()
            ))
        })?;
        let file_name = item.file_name();
        let name = file_name.to_string_lossy();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let rel_child = if relative_dir.as_os_str().is_empty() {
            PathBuf::from(file_name)
        } else {
            relative_dir.join(file_name)
        };
        if let Some(scope) = ctx.scope {
            let rel_str = relative_display(&rel_child);
            if !scope.filesystem.can_read(&rel_str) {
                continue;
            }
        }

        let metadata = item.metadata().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read metadata for '{}': {error}",
                item.path().display()
            ))
        })?;
        let is_dir = metadata.is_dir();
        let byte_size = if is_dir { 0 } else { metadata.len() };
        entries.push(ListFilesEntry {
            path: relative_display(&rel_child),
            is_dir,
            byte_size,
        });

        if recursive && is_dir {
            list_entries(ctx, &item.path(), &rel_child, true, include_hidden, entries)?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteFileInput {
    path: String,
    content: String,
    #[serde(default)]
    append: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteFileOutput {
    path: String,
    bytes_written: u64,
    append: bool,
}

fn handle_write_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<WriteFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_write(ctx.scope, &rel)?;

    let absolute = ctx.worktree.join(&rel);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to create parent directory '{}': {error}",
                parent.display()
            ))
        })?;
    }

    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if input.append {
        options.append(true);
    } else {
        options.truncate(true);
    }

    let mut file = options.open(&absolute).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to open '{}': {error}",
            absolute.display()
        ))
    })?;
    file.write_all(input.content.as_bytes()).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to write '{}': {error}",
            absolute.display()
        ))
    })?;

    let output = WriteFileOutput {
        path: relative_display(&rel),
        bytes_written: u64::try_from(input.content.len()).unwrap_or(u64::MAX),
        append: input.append,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode write_file output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunCommandInput {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunCommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn handle_run_command(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<RunCommandInput>(input)?;
    let command = input.command.trim();
    if command.is_empty() {
        return Err(NativeToolEngineError::validation(
            "run_command requires a non-empty command",
        ));
    }

    let command_line = if input.args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {}", input.args.join(" "))
    };

    if let Some(scope) = ctx.scope {
        if !scope.execution.is_allowed(&command_line) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "run_command blocked by execution scope: {command_line}"
            )));
        }
    }
    if !ctx.command_policy.is_allowed(&command_line) {
        return Err(NativeToolEngineError::policy_violation(format!(
            "run_command blocked by native policy: {command_line}"
        )));
    }

    let timeout_ms = input
        .timeout_ms
        .unwrap_or(default_timeout_ms)
        .min(default_timeout_ms);
    let started = Instant::now();
    let output = Command::new(command)
        .args(&input.args)
        .current_dir(ctx.worktree)
        .output()
        .map_err(|error| {
            NativeToolEngineError::execution(format!("failed to execute '{command_line}': {error}"))
        })?;
    if started.elapsed() > Duration::from_millis(timeout_ms) {
        return Err(NativeToolEngineError::timeout("run_command", timeout_ms));
    }

    let status = output.status.code().unwrap_or(-1);
    let result = RunCommandOutput {
        exit_code: status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        timed_out: false,
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode run_command output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitDiffInput {
    #[serde(default)]
    staged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitStatusOutput {
    output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitDiffOutput {
    output: String,
}

fn handle_git_status(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let _ = decode_input::<NoInput>(input)?;
    ensure_repository_read(ctx.scope)?;
    let output = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(ctx.worktree)
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git status failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitStatusOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_status output: {error}"))
    })
}

fn handle_git_diff(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GitDiffInput>(input)?;
    ensure_repository_read(ctx.scope)?;

    let mut args = vec!["diff"];
    if input.staged {
        args.push("--staged");
    }
    args.push("--no-ext-diff");

    let output = Command::new("git")
        .args(&args)
        .current_dir(ctx.worktree)
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git diff failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitDiffOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_diff output: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scope::{ExecutionScope, FilePermission, FilesystemScope, PathRule, Scope};
    use proptest::prelude::*;

    fn allow_all_scope() -> Scope {
        Scope::new()
            .with_filesystem(
                FilesystemScope::new().with_rule(PathRule::new("*", FilePermission::Write)),
            )
            .with_execution(ExecutionScope::new().allow("*"))
    }

    #[test]
    fn rejects_unknown_tool_names() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "nope".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({}),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("unknown tool should fail");
        assert_eq!(error.code, "native_tool_unknown");
    }

    #[test]
    fn rejects_invalid_input_schema() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "read_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "missing": "path" }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("invalid schema should fail");
        assert_eq!(error.code, "native_tool_input_invalid");
    }

    #[test]
    fn write_file_obeys_scope_gate() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src/", FilePermission::Read)),
        );
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "write_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "path": "src/main.rs", "content": "fn main() {}" }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("write should be blocked");
        assert_eq!(error.code, "native_scope_violation");
    }

    #[test]
    fn run_command_is_deny_by_default() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("policy should deny command");
        assert_eq!(error.code, "native_policy_violation");
    }

    #[test]
    fn run_command_respects_allowlist_policy() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let value = engine
            .execute(&action, &ctx)
            .expect("allowlisted command should run");
        let output: RunCommandOutput =
            serde_json::from_value(value).expect("run_command output should decode");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    proptest! {
        #[test]
        fn replay_is_deterministic_for_write_then_read(
            file_name in "[a-z]{1,8}",
            content in "[ -~]{0,64}"
        ) {
            let engine = NativeToolEngine::default();
            let scope = allow_all_scope();
            let policy = NativeCommandPolicy {
                allowlist: vec!["echo".to_string()],
                denylist: vec!["rm".to_string()],
                deny_by_default: true,
            };

            let run_once = |root: &Path| -> (Value, Value) {
                let ctx = ToolExecutionContext {
                    worktree: root,
                    scope: Some(&scope),
                    command_policy: &policy,
                };
                let relative_path = format!("src/{file_name}.txt");
                let write = NativeToolAction {
                    name: "write_file".to_string(),
                    version: TOOL_VERSION_V1.to_string(),
                    input: json!({"path": relative_path, "content": content}),
                };
                let read = NativeToolAction {
                    name: "read_file".to_string(),
                    version: TOOL_VERSION_V1.to_string(),
                    input: json!({"path": format!("src/{file_name}.txt")}),
                };
                let write_out = engine.execute(&write, &ctx).expect("write must pass");
                let read_out = engine.execute(&read, &ctx).expect("read must pass");
                (write_out, read_out)
            };

            let tmp_a = tempfile::tempdir().expect("tempdir");
            let tmp_b = tempfile::tempdir().expect("tempdir");
            let first = run_once(tmp_a.path());
            let second = run_once(tmp_b.path());
            prop_assert_eq!(first, second);
        }
    }

    #[test]
    fn dispatch_overhead_baseline_is_bounded() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("README.md"), "hello").expect("seed file");
        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "list_files".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "path": ".", "recursive": false }),
        };

        let samples = 200_u32;
        let started = Instant::now();
        for _ in 0..samples {
            let _ = engine
                .execute(&action, &ctx)
                .expect("dispatch should succeed");
        }
        let avg_us = started.elapsed().as_micros() / u128::from(samples);
        assert!(
            avg_us < 50_000,
            "dispatch baseline too slow: average {avg_us}us"
        );
    }

    #[test]
    fn validation_latency_baseline_is_bounded() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
        };
        let action = NativeToolAction {
            name: "read_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "oops": "invalid" }),
        };

        let samples = 500_u32;
        let started = Instant::now();
        for _ in 0..samples {
            let error = engine
                .execute(&action, &ctx)
                .expect_err("invalid payload should fail");
            assert_eq!(error.code, "native_tool_input_invalid");
        }
        let avg_us = started.elapsed().as_micros() / u128::from(samples);
        assert!(
            avg_us < 20_000,
            "validation baseline too slow: average {avg_us}us"
        );
    }
}
