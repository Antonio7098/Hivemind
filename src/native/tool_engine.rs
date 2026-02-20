//! Native typed tool engine with schema validation and policy-aware dispatch.
//!
//! Sprint 44 introduces a deterministic tool registry for native runtime turns.

use crate::adapters::runtime::{NativeToolCallFailure, NativeToolCallTrace};
use crate::core::graph_query::{
    load_partition_paths_from_constitution, GraphQueryBounds, GraphQueryIndex,
    GraphQueryRepository, GraphQueryRequest, GraphQueryResult, GRAPH_QUERY_ENV_CONSTITUTION_PATH,
    GRAPH_QUERY_ENV_GATE_ERROR, GRAPH_QUERY_ENV_SNAPSHOT_PATH, GRAPH_QUERY_REFRESH_HINT,
};
use crate::core::scope::Scope;
use jsonschema::JSONSchema;
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use ucp_api::{canonical_fingerprint, PortableDocument, CODEGRAPH_PROFILE_MARKER};

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
    pub env: &'a HashMap<String, String>,
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
        engine.register_builtin::<GraphQueryInput, GraphQueryResult>(
            "graph_query",
            "graph_query_read",
            vec![ToolPermission::FilesystemRead, ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum GraphQueryKindInput {
    Neighbors,
    Dependents,
    Subgraph,
    Filter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GraphQueryInput {
    kind: GraphQueryKindInput,
    #[serde(default)]
    node: Option<String>,
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    edge_types: Vec<String>,
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    path_prefix: Option<String>,
    #[serde(default)]
    partition: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

impl GraphQueryInput {
    fn into_request(self) -> Result<GraphQueryRequest, NativeToolEngineError> {
        match self.kind {
            GraphQueryKindInput::Neighbors => Ok(GraphQueryRequest::Neighbors {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.neighbors requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Dependents => Ok(GraphQueryRequest::Dependents {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.dependents requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Subgraph => Ok(GraphQueryRequest::Subgraph {
                seed: self.seed.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'seed'")
                })?,
                depth: self.depth.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'depth'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Filter => Ok(GraphQueryRequest::Filter {
                node_type: self.node_type,
                path_prefix: self.path_prefix,
                partition: self.partition,
                max_results: self.max_results,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphQueryGateError {
    code: String,
    message: String,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotCommit {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotProvenance {
    head_commits: Vec<RuntimeGraphSnapshotCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotRepository {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
    canonical_fingerprint: String,
    document: PortableDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotArtifact {
    profile_version: String,
    canonical_fingerprint: String,
    provenance: RuntimeGraphSnapshotProvenance,
    repositories: Vec<RuntimeGraphSnapshotRepository>,
}

fn graph_query_runtime_error(code: &str, message: impl Into<String>) -> NativeToolEngineError {
    NativeToolEngineError::new(code, message, false)
}

fn graph_query_runtime_error_with_refresh_hint(
    code: &str,
    message: impl Into<String>,
) -> NativeToolEngineError {
    graph_query_runtime_error(
        code,
        format!("{}. Hint: {GRAPH_QUERY_REFRESH_HINT}", message.into()),
    )
}

fn resolve_repo_head_commit(repo_path: &Path) -> Result<String, NativeToolEngineError> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "failed to resolve git HEAD for '{}': {error}",
                    repo_path.display()
                ),
            )
        })?;
    if !output.status.success() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!(
                "failed to resolve git HEAD for '{}': {}",
                repo_path.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!("resolved empty HEAD for '{}'", repo_path.display()),
        ));
    }
    Ok(head)
}

fn graph_query_bounds_from_env(
    env: &HashMap<String, String>,
) -> Result<GraphQueryBounds, NativeToolEngineError> {
    let mut bounds = GraphQueryBounds::default();
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT") {
        bounds.max_results_limit = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH") {
        bounds.max_subgraph_depth = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS") {
        bounds.default_max_results = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS '{raw}': {error}"
            ))
        })?;
    }
    if bounds.max_results_limit == 0
        || bounds.max_subgraph_depth == 0
        || bounds.default_max_results == 0
    {
        return Err(NativeToolEngineError::execution(
            "graph query bounds configuration values must be > 0",
        ));
    }
    if bounds.default_max_results > bounds.max_results_limit {
        return Err(NativeToolEngineError::execution(format!(
            "graph query default max results {} exceeds max limit {}",
            bounds.default_max_results, bounds.max_results_limit
        )));
    }
    Ok(bounds)
}

fn load_runtime_graph_snapshot(
    env: &HashMap<String, String>,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    if let Some(raw) = env.get(GRAPH_QUERY_ENV_GATE_ERROR) {
        let gate = serde_json::from_str::<RuntimeGraphQueryGateError>(raw).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to decode {GRAPH_QUERY_ENV_GATE_ERROR}: {error}"
            ))
        })?;
        let message = gate.hint.as_ref().map_or_else(
            || gate.message.clone(),
            |hint| format!("{}. Hint: {hint}", gate.message),
        );
        return Err(graph_query_runtime_error(&gate.code, message));
    }

    let snapshot_path = env
        .get(GRAPH_QUERY_ENV_SNAPSHOT_PATH)
        .ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_missing",
                format!("missing runtime env {GRAPH_QUERY_ENV_SNAPSHOT_PATH}"),
            )
        })?
        .clone();
    let raw = fs::read_to_string(&snapshot_path).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("failed to read graph snapshot '{snapshot_path}': {error}"),
        )
    })?;
    if raw.trim().is_empty() || raw.trim() == "{}" {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("graph snapshot '{snapshot_path}' is empty"),
        ));
    }

    serde_json::from_str::<RuntimeGraphSnapshotArtifact>(&raw).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid graph snapshot '{snapshot_path}': {error}"),
        )
    })
}

fn validate_runtime_graph_snapshot(
    artifact: &RuntimeGraphSnapshotArtifact,
) -> Result<(), NativeToolEngineError> {
    if artifact.profile_version != CODEGRAPH_PROFILE_MARKER {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_profile_mismatch",
            format!(
                "snapshot profile '{}' is not supported; expected '{}'",
                artifact.profile_version, CODEGRAPH_PROFILE_MARKER
            ),
        ));
    }

    let head_by_repo: HashMap<(String, String), String> = artifact
        .provenance
        .head_commits
        .iter()
        .map(|entry| {
            (
                (entry.repo_name.clone(), entry.repo_path.clone()),
                entry.commit_hash.clone(),
            )
        })
        .collect();

    for repo in &artifact.repositories {
        let document = repo.document.to_document().map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "invalid portable document for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        let computed = canonical_fingerprint(&document).map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "failed to compute fingerprint for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        if computed != repo.canonical_fingerprint {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!("fingerprint mismatch for repository '{}'", repo.repo_name),
            ));
        }

        let key = (repo.repo_name.clone(), repo.repo_path.clone());
        let recorded_head = head_by_repo.get(&key).ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "missing head commit provenance for repository '{}'",
                    repo.repo_name
                ),
            )
        })?;
        if recorded_head != &repo.commit_hash {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "snapshot repository commit mismatch for '{}' (snapshot={} provenance={})",
                    repo.repo_name, repo.commit_hash, recorded_head
                ),
            ));
        }

        let current_head = resolve_repo_head_commit(Path::new(&repo.repo_path))?;
        if current_head != *recorded_head {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "graph snapshot is stale for repository '{}' (snapshot={} current={})",
                    repo.repo_name, recorded_head, current_head
                ),
            ));
        }
    }

    let aggregate = aggregate_snapshot_fingerprint_registry_style(&artifact.repositories);
    if aggregate != artifact.canonical_fingerprint {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_integrity_invalid",
            "aggregate graph snapshot fingerprint mismatch",
        ));
    }

    Ok(())
}

fn aggregate_snapshot_fingerprint_registry_style(
    repositories: &[RuntimeGraphSnapshotRepository],
) -> String {
    let mut entries = repositories
        .iter()
        .map(|repo| {
            format!(
                "{}|{}|{}|{}",
                repo.repo_name, repo.repo_path, repo.commit_hash, repo.canonical_fingerprint
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    let joined = entries.join("\n");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    joined.as_bytes().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn map_graph_query_error(
    error: crate::core::graph_query::GraphQueryError,
) -> NativeToolEngineError {
    NativeToolEngineError::new(error.code, error.message, false)
}

fn handle_graph_query(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GraphQueryInput>(input)?;
    let request = input.into_request()?;
    let bounds = graph_query_bounds_from_env(ctx.env)?;
    let artifact = load_runtime_graph_snapshot(ctx.env)?;
    validate_runtime_graph_snapshot(&artifact)?;

    let partition_paths = ctx
        .env
        .get(GRAPH_QUERY_ENV_CONSTITUTION_PATH)
        .map(PathBuf::from)
        .as_deref()
        .map_or_else(BTreeMap::new, load_partition_paths_from_constitution);

    let repositories = artifact
        .repositories
        .iter()
        .map(|repo| GraphQueryRepository {
            repo_name: repo.repo_name.clone(),
            document: repo.document.clone(),
        })
        .collect::<Vec<_>>();
    let index = GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
        .map_err(map_graph_query_error)?;

    let started = Instant::now();
    let mut result = index
        .execute(&request, &artifact.canonical_fingerprint, &bounds)
        .map_err(map_graph_query_error)?;
    result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode graph_query output: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scope::{ExecutionScope, FilePermission, FilesystemScope, PathRule, Scope};
    use proptest::prelude::*;
    use ucp_api::{
        build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig,
        CODEGRAPH_EXTRACTOR_VERSION,
    };

    fn init_git_repo(path: &Path) {
        fs::create_dir_all(path).expect("create repo dir");
        let output = Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_commit_all(path: &Path, message: &str) {
        let add = Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("git add");
        assert!(
            add.status.success(),
            "{}",
            String::from_utf8_lossy(&add.stderr)
        );
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                message,
            ])
            .current_dir(path)
            .output()
            .expect("git commit");
        assert!(
            commit.status.success(),
            "{}",
            String::from_utf8_lossy(&commit.stderr)
        );
    }

    fn git_head(path: &Path) -> String {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .expect("git rev-parse");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write_snapshot_artifact(repo_path: &Path, snapshot_path: &Path) {
        let commit_hash = git_head(repo_path);
        let built = build_code_graph(&CodeGraphBuildInput {
            repository_path: repo_path.to_path_buf(),
            commit_hash: commit_hash.clone(),
            config: CodeGraphExtractorConfig::default(),
        })
        .expect("build code graph");
        let portable = PortableDocument::from_document(&built.document);
        let repositories = vec![RuntimeGraphSnapshotRepository {
            repo_name: "repo".to_string(),
            repo_path: repo_path.to_string_lossy().to_string(),
            commit_hash: commit_hash.clone(),
            canonical_fingerprint: built.canonical_fingerprint.clone(),
            document: portable,
        }];
        let artifact = RuntimeGraphSnapshotArtifact {
            profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            canonical_fingerprint: aggregate_snapshot_fingerprint_registry_style(&repositories),
            provenance: RuntimeGraphSnapshotProvenance {
                head_commits: vec![RuntimeGraphSnapshotCommit {
                    repo_name: "repo".to_string(),
                    repo_path: repo_path.to_string_lossy().to_string(),
                    commit_hash,
                }],
            },
            repositories,
        };
        let raw = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "graph_snapshot.v1",
            "snapshot_version": 1,
            "provenance": {
                "project_id": uuid::Uuid::new_v4(),
                "head_commits": artifact.provenance.head_commits,
                "generated_at": chrono::Utc::now(),
            },
            "ucp_engine_version": CODEGRAPH_EXTRACTOR_VERSION,
            "profile_version": artifact.profile_version,
            "canonical_fingerprint": artifact.canonical_fingerprint,
            "summary": {
                "total_nodes": built.stats.total_nodes,
                "repository_nodes": built.stats.repository_nodes,
                "directory_nodes": built.stats.directory_nodes,
                "file_nodes": built.stats.file_nodes,
                "symbol_nodes": built.stats.symbol_nodes,
                "total_edges": built.stats.total_edges,
                "reference_edges": built.stats.reference_edges,
                "export_edges": built.stats.export_edges,
                "languages": built.stats.languages,
            },
            "repositories": artifact.repositories,
            "static_projection": "",
        }))
        .expect("serialize snapshot");
        fs::write(snapshot_path, raw).expect("write snapshot");
    }

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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
        let env = HashMap::new();
        let scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src/", FilePermission::Read)),
        );
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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

    #[test]
    fn graph_query_tool_reads_snapshot_with_bounds() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        init_git_repo(&repo);
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
        fs::write(repo.join("src/main.rs"), "fn main() { helper(); }\n").expect("write main");
        git_commit_all(&repo, "seed");

        let snapshot_path = tmp.path().join("graph_snapshot.json");
        write_snapshot_artifact(&repo, &snapshot_path);

        let constitution_path = tmp.path().join("constitution.yaml");
        fs::write(
            &constitution_path,
            "partitions:\n  - id: core\n    path: src\n",
        )
        .expect("write constitution");

        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let mut env = HashMap::new();
        env.insert(
            GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
            snapshot_path.to_string_lossy().to_string(),
        );
        env.insert(
            GRAPH_QUERY_ENV_CONSTITUTION_PATH.to_string(),
            constitution_path.to_string_lossy().to_string(),
        );
        let ctx = ToolExecutionContext {
            worktree: repo.as_path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
        };
        let action = NativeToolAction {
            name: "graph_query".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "kind": "filter",
                "node_type": "file",
                "path_prefix": "src",
                "max_results": 50
            }),
        };

        let value = engine
            .execute(&action, &ctx)
            .expect("graph query should run");
        let result: GraphQueryResult = serde_json::from_value(value).expect("decode result");
        assert_eq!(result.query_kind, "filter");
        assert!(
            !result.nodes.is_empty(),
            "{}",
            serde_json::to_string(&result).unwrap_or_default()
        );
        assert!(result.duration_ms <= 5_000);
    }

    #[test]
    fn graph_query_tool_fails_when_snapshot_is_stale() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        init_git_repo(&repo);
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
        git_commit_all(&repo, "seed");

        let snapshot_path = tmp.path().join("graph_snapshot.json");
        write_snapshot_artifact(&repo, &snapshot_path);

        fs::write(
            repo.join("src/lib.rs"),
            "pub fn helper() { println!(\"x\"); }\n",
        )
        .expect("mutate repo");
        git_commit_all(&repo, "stale");

        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let mut env = HashMap::new();
        env.insert(
            GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
            snapshot_path.to_string_lossy().to_string(),
        );
        let ctx = ToolExecutionContext {
            worktree: repo.as_path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
        };
        let action = NativeToolAction {
            name: "graph_query".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "kind": "filter",
                "max_results": 10
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("stale snapshot should fail");
        assert_eq!(error.code, "graph_snapshot_stale");
        assert!(error.message.contains("hivemind graph snapshot refresh"));
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
                let env = HashMap::new();
                let ctx = ToolExecutionContext {
                    worktree: root,
                    scope: Some(&scope),
                    command_policy: &policy,
                    env: &env,
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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = ToolExecutionContext {
            worktree: tmp.path(),
            scope: Some(&scope),
            command_policy: &policy,
            env: &env,
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
