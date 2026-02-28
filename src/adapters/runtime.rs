//! Runtime adapter interface for execution backends.
//!
//! Runtime adapters are the only components that interact directly with
//! execution runtimes (e.g., Claude Code, `OpenCode`, Codex CLI). They provide
//! a uniform interface for task execution regardless of the underlying agent.
//!
//! # Core Types
//!
//! - [`RuntimeAdapter`] - Trait that all adapters must implement
//! - [`ExecutionInput`] - Input for a task execution (description, criteria, context)
//! - [`ExecutionReport`] - Result of an execution (exit code, output, file changes)
//! - [`AdapterConfig`] - Configuration for an adapter (binary path, timeout, env)
//!
//! # Adapter Lifecycle
//!
//! ```text
//! initialize() → prepare(task_id, worktree) → execute(input) → terminate()
//! ```
//!
//! 1. **Initialize**: Set up the adapter (validate binary, configure environment)
//! 2. **Prepare**: Set up for a specific task (create worktree, configure scope)
//! 3. **Execute**: Run the task and collect results
//! 4. **Terminate**: Clean up resources
//!
//! # Example: Using an Adapter
//!
//! ```no_run
//! use hivemind::adapters::runtime::{AdapterConfig, ExecutionInput, RuntimeAdapter};
//! use std::path::PathBuf;
//! use std::time::Duration;
//!
//! // Configure the adapter
//! let config = AdapterConfig::new("opencode", PathBuf::from("/usr/local/bin/opencode"))
//!     .with_timeout(Duration::from_secs(300))
//!     .with_env("MODEL", "claude-3-opus");
//!
//! // Prepare input
//! let input = ExecutionInput {
//!     task_description: "Implement user authentication".to_string(),
//!     success_criteria: "Users can log in and log out".to_string(),
//!     context: Some("Use JWT tokens".to_string()),
//!     prior_attempts: vec![],
//!     verifier_feedback: None,
//! };
//! ```
//!
//! # Built-in Adapters
//!
//! See the adapter implementations:
//! - [`opencode`](super::opencode) - `OpenCode` adapter (primary)
//! - [`claude_code`](super::claude_code) - Claude Code adapter
//! - [`codex`](super::codex) - Codex CLI adapter
//! - [`kilo`](super::kilo) - Kilo adapter

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::hash::BuildHasher;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

const ENV_POLICY_INHERIT_MODE_KEY: &str = "HIVEMIND_RUNTIME_ENV_INHERIT";
const ENV_POLICY_INHERIT_ALL: &str = "all";
const ENV_POLICY_INHERIT_CORE: &str = "core";
const ENV_POLICY_INHERIT_NONE: &str = "none";
const ENV_POLICY_INTERNAL_RESERVED_PREFIXES: [&str; 7] = [
    "HIVEMIND_TASK_",
    "HIVEMIND_ATTEMPT_",
    "HIVEMIND_FLOW_",
    "HIVEMIND_REPO_",
    "HIVEMIND_GRAPH_",
    "HIVEMIND_SCOPE_TRACE_",
    "HIVEMIND_RUNTIME_INTERNAL_",
];
const ENV_POLICY_INTERNAL_RESERVED_KEYS: [&str; 6] = [
    "HIVEMIND_PRIMARY_WORKTREE",
    "HIVEMIND_ALL_WORKTREES",
    "HIVEMIND_TASK_SCOPE_JSON",
    "HIVEMIND_BIN",
    "HIVEMIND_AGENT_BIN",
    "HIVEMIND_PROJECT_CONSTITUTION_PATH",
];
const ENV_POLICY_CORE_INHERITED_KEYS: [&str; 18] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SYSTEMROOT",
    "COMSPEC",
    "PATHEXT",
    "WINDIR",
    "NUMBER_OF_PROCESSORS",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEnvInheritMode {
    All,
    Core,
    None,
}

impl RuntimeEnvInheritMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => ENV_POLICY_INHERIT_ALL,
            Self::Core => ENV_POLICY_INHERIT_CORE,
            Self::None => ENV_POLICY_INHERIT_NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEnvBuildError {
    pub code: String,
    pub message: String,
}

impl RuntimeEnvBuildError {
    #[must_use]
    pub fn invalid_inherit_mode(raw: &str) -> Self {
        Self {
            code: "runtime_env_inherit_mode_invalid".to_string(),
            message: format!(
                "Invalid {ENV_POLICY_INHERIT_MODE_KEY} value '{raw}'. Expected one of: {ENV_POLICY_INHERIT_ALL}, {ENV_POLICY_INHERIT_CORE}, {ENV_POLICY_INHERIT_NONE}"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEnvironmentProvenance {
    pub inherit_mode: RuntimeEnvInheritMode,
    #[serde(default)]
    pub inherited_keys: Vec<String>,
    #[serde(default)]
    pub overlay_keys: Vec<String>,
    #[serde(default)]
    pub explicit_sensitive_overlay_keys: Vec<String>,
    #[serde(default)]
    pub dropped_sensitive_inherited_keys: Vec<String>,
    #[serde(default)]
    pub dropped_reserved_inherited_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedRuntimeEnvironment {
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub provenance: RuntimeEnvironmentProvenance,
}

fn parse_inherit_mode<S: BuildHasher>(
    runtime_overlay: &HashMap<String, String, S>,
) -> std::result::Result<RuntimeEnvInheritMode, RuntimeEnvBuildError> {
    let Some(raw) = runtime_overlay.get(ENV_POLICY_INHERIT_MODE_KEY) else {
        return Ok(RuntimeEnvInheritMode::Core);
    };
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        ENV_POLICY_INHERIT_ALL => Ok(RuntimeEnvInheritMode::All),
        ENV_POLICY_INHERIT_CORE => Ok(RuntimeEnvInheritMode::Core),
        ENV_POLICY_INHERIT_NONE => Ok(RuntimeEnvInheritMode::None),
        _ => Err(RuntimeEnvBuildError::invalid_inherit_mode(raw)),
    }
}

fn is_core_inherited_key(key: &str) -> bool {
    ENV_POLICY_CORE_INHERITED_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("KEY") || upper.contains("SECRET") || upper.contains("TOKEN")
}

fn is_reserved_internal_key(key: &str) -> bool {
    ENV_POLICY_INTERNAL_RESERVED_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
        || ENV_POLICY_INTERNAL_RESERVED_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

#[must_use]
pub fn deterministic_env_pairs<S: BuildHasher>(
    env: &HashMap<String, String, S>,
) -> Vec<(String, String)> {
    let mut ordered = BTreeMap::new();
    for (key, value) in env {
        ordered.insert(key.clone(), value.clone());
    }
    ordered.into_iter().collect()
}

pub fn build_protected_runtime_environment<S: BuildHasher>(
    runtime_overlay: &HashMap<String, String, S>,
) -> std::result::Result<ProtectedRuntimeEnvironment, RuntimeEnvBuildError> {
    let inherit_mode = parse_inherit_mode(runtime_overlay)?;

    let inherited_candidates: Vec<(String, String)> = match inherit_mode {
        RuntimeEnvInheritMode::All => std::env::vars().collect(),
        RuntimeEnvInheritMode::Core => std::env::vars()
            .filter(|(key, _)| is_core_inherited_key(key))
            .collect(),
        RuntimeEnvInheritMode::None => Vec::new(),
    };

    let mut ordered_env = BTreeMap::new();
    let mut inherited_keys = Vec::new();
    let mut overlay_keys = Vec::new();
    let mut explicit_sensitive_overlay_keys = Vec::new();
    let mut dropped_sensitive_inherited_keys = Vec::new();
    let mut dropped_reserved_inherited_keys = Vec::new();

    let mut inherited_sorted = inherited_candidates;
    inherited_sorted.sort_by(|a, b| a.0.cmp(&b.0));

    for (key, value) in inherited_sorted {
        if is_reserved_internal_key(&key) {
            dropped_reserved_inherited_keys.push(key);
            continue;
        }
        if is_sensitive_key(&key) {
            dropped_sensitive_inherited_keys.push(key);
            continue;
        }
        inherited_keys.push(key.clone());
        ordered_env.insert(key, value);
    }

    for (key, value) in deterministic_env_pairs(runtime_overlay) {
        if key.eq_ignore_ascii_case(ENV_POLICY_INHERIT_MODE_KEY) {
            continue;
        }
        if is_sensitive_key(&key) {
            explicit_sensitive_overlay_keys.push(key.clone());
        }
        overlay_keys.push(key.clone());
        ordered_env.insert(key, value);
    }

    let env = ordered_env.into_iter().collect::<HashMap<_, _>>();
    Ok(ProtectedRuntimeEnvironment {
        env,
        provenance: RuntimeEnvironmentProvenance {
            inherit_mode,
            inherited_keys,
            overlay_keys,
            explicit_sensitive_overlay_keys,
            dropped_sensitive_inherited_keys,
            dropped_reserved_inherited_keys,
        },
    })
}

/// Input for runtime execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionInput {
    /// Task description/objective.
    pub task_description: String,
    /// Success criteria.
    pub success_criteria: String,
    /// Additional context.
    pub context: Option<String>,
    /// Prior attempt summaries (for retries).
    pub prior_attempts: Vec<AttemptSummary>,
    /// Verifier feedback (for retries).
    pub verifier_feedback: Option<String>,
}

/// Formats an execution input into the runtime prompt payload.
#[must_use]
pub fn format_execution_prompt(input: &ExecutionInput) -> String {
    let task_description = &input.task_description;
    let success_criteria = &input.success_criteria;
    let mut prompt = format!("Task: {task_description}\n\n");
    let _ = write!(prompt, "Success Criteria: {success_criteria}\n\n");

    if let Some(ref context) = input.context {
        let _ = write!(prompt, "Context:\n{context}\n\n");
    }

    if !input.prior_attempts.is_empty() {
        prompt.push_str("Prior Attempts:\n");
        for attempt in &input.prior_attempts {
            let attempt_number = attempt.attempt_number;
            let summary = &attempt.summary;
            let _ = writeln!(prompt, "- Attempt {attempt_number}: {summary}");
            if let Some(ref reason) = attempt.failure_reason {
                let _ = writeln!(prompt, "  Failure: {reason}");
            }
        }
        prompt.push('\n');
    }

    if let Some(ref feedback) = input.verifier_feedback {
        let _ = write!(prompt, "Verifier Feedback:\n{feedback}\n\n");
    }

    prompt
}

/// Summary of a prior attempt for context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptSummary {
    /// Attempt number.
    pub attempt_number: u32,
    /// What was tried.
    pub summary: String,
    /// Why it failed.
    pub failure_reason: Option<String>,
}

/// Payload capture mode for native runtime traces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NativePayloadCaptureMode {
    #[default]
    MetadataOnly,
    FullPayload,
}

/// Native runtime invocation provenance and turn trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInvocationTrace {
    pub invocation_id: String,
    pub provider: String,
    pub model: String,
    pub runtime_version: String,
    #[serde(default)]
    pub capture_mode: NativePayloadCaptureMode,
    #[serde(default)]
    pub turns: Vec<NativeTurnTrace>,
    pub final_state: String,
    #[serde(default)]
    pub final_summary: Option<String>,
    #[serde(default)]
    pub failure: Option<NativeInvocationFailure>,
    #[serde(default)]
    pub transport: NativeTransportTelemetry,
}

/// One native runtime turn trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTurnTrace {
    pub turn_index: u32,
    pub from_state: String,
    pub to_state: String,
    pub model_request: String,
    pub model_response: String,
    #[serde(default)]
    pub tool_calls: Vec<NativeToolCallTrace>,
    #[serde(default)]
    pub turn_summary: Option<String>,
}

/// One native runtime tool-call trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolCallTrace {
    pub call_id: String,
    pub tool_name: String,
    pub request: String,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub failure: Option<NativeToolCallFailure>,
    #[serde(default)]
    pub policy_tags: Vec<String>,
}

/// Native tool-call failure payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolCallFailure {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// Native invocation failure payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeInvocationFailure {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub recovery_hint: Option<String>,
}

/// Native model transport telemetry for retries/fallbacks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NativeTransportTelemetry {
    #[serde(default)]
    pub attempts: Vec<NativeTransportAttemptTrace>,
    #[serde(default)]
    pub fallback_activations: Vec<NativeTransportFallbackTrace>,
    #[serde(default)]
    pub active_transport: Option<String>,
}

/// One transport attempt failure/retry decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTransportAttemptTrace {
    pub turn_index: u32,
    pub attempt: u32,
    pub transport: String,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub rate_limited: bool,
    #[serde(default)]
    pub status_code: Option<u16>,
    #[serde(default)]
    pub backoff_ms: Option<u64>,
}

/// One transport fallback activation record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeTransportFallbackTrace {
    pub turn_index: u32,
    pub from_transport: String,
    pub to_transport: String,
    pub reason: String,
}

/// Report from runtime execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    /// Exit code from the runtime.
    pub exit_code: i32,
    /// Execution duration.
    pub duration: Duration,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Files created during execution.
    pub files_created: Vec<PathBuf>,
    /// Files modified during execution.
    pub files_modified: Vec<PathBuf>,
    /// Files deleted during execution.
    pub files_deleted: Vec<PathBuf>,
    /// Any errors that occurred.
    pub errors: Vec<RuntimeError>,
    /// Native runtime turn-level invocation trace (when available).
    #[serde(default)]
    pub native_invocation: Option<NativeInvocationTrace>,
}

impl ExecutionReport {
    /// Creates a successful execution report.
    pub fn success(duration: Duration, stdout: String, stderr: String) -> Self {
        Self {
            exit_code: 0,
            duration,
            stdout,
            stderr,
            files_created: Vec::new(),
            files_modified: Vec::new(),
            files_deleted: Vec::new(),
            errors: Vec::new(),
            native_invocation: None,
        }
    }

    /// Creates a failed execution report.
    pub fn failure(exit_code: i32, duration: Duration, error: RuntimeError) -> Self {
        Self::failure_with_output(exit_code, duration, error, String::new(), String::new())
    }

    /// Creates a failed execution report preserving runtime output.
    pub fn failure_with_output(
        exit_code: i32,
        duration: Duration,
        error: RuntimeError,
        stdout: String,
        stderr: String,
    ) -> Self {
        Self {
            exit_code,
            duration,
            stdout,
            stderr,
            files_created: Vec::new(),
            files_modified: Vec::new(),
            files_deleted: Vec::new(),
            errors: vec![error],
            native_invocation: None,
        }
    }

    /// Returns true if execution succeeded (exit code 0, no errors).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0 && self.errors.is_empty()
    }

    /// Adds file changes to the input.
    #[must_use]
    pub fn with_file_changes(
        mut self,
        created: Vec<PathBuf>,
        modified: Vec<PathBuf>,
        deleted: Vec<PathBuf>,
    ) -> Self {
        self.files_created = created;
        self.files_modified = modified;
        self.files_deleted = deleted;
        self
    }

    /// Attaches native invocation trace details to this report.
    #[must_use]
    pub fn with_native_invocation(mut self, invocation: NativeInvocationTrace) -> Self {
        self.native_invocation = Some(invocation);
        self
    }
}

/// Interactive runtime transport events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractiveAdapterEvent {
    /// Runtime emitted output content.
    Output { content: String },
    /// User input forwarded to runtime.
    Input { content: String },
    /// Runtime interrupted (for example by Ctrl+C).
    Interrupted,
}

/// Interactive execution result metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveExecutionResult {
    /// Final execution report.
    pub report: ExecutionReport,
    /// Optional termination reason.
    pub terminated_reason: Option<String>,
}

/// An error from the runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeError {
    /// Error code.
    pub code: String,
    /// Error message.
    pub message: String,
    /// Whether this error is recoverable.
    pub recoverable: bool,
}

impl RuntimeError {
    /// Creates a new runtime error.
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }

    /// Creates a timeout error.
    pub fn timeout(duration: Duration) -> Self {
        Self::new(
            "timeout",
            format!("Execution timed out after {duration:?}"),
            true,
        )
    }

    /// Creates a crash error.
    pub fn crash(message: impl Into<String>) -> Self {
        Self::new("crash", message, false)
    }
}

/// Configuration for a runtime adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// Name of the adapter.
    pub name: String,
    /// Path to the runtime binary.
    pub binary_path: PathBuf,
    /// Additional arguments to pass.
    pub args: Vec<String>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Execution timeout.
    pub timeout: Duration,
    /// Working directory (if different from worktree).
    pub working_dir: Option<PathBuf>,
}

impl AdapterConfig {
    /// Creates a new adapter config.
    pub fn new(name: impl Into<String>, binary_path: PathBuf) -> Self {
        Self {
            name: name.into(),
            binary_path,
            args: Vec::new(),
            env: HashMap::new(),
            timeout: Duration::from_secs(300), // 5 minutes default
            working_dir: None,
        }
    }

    /// Sets the timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Adds an argument.
    #[must_use]
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Sets an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

/// Trait for runtime adapters.
///
/// All adapters must implement this trait to be usable by Hivemind.
pub trait RuntimeAdapter: Send + Sync {
    /// Returns the adapter name.
    fn name(&self) -> &str;

    /// Initializes the adapter.
    fn initialize(&mut self) -> Result<(), RuntimeError>;

    /// Prepares the adapter for execution.
    fn prepare(&mut self, task_id: Uuid, worktree: &Path) -> Result<(), RuntimeError>;

    /// Executes the runtime with the given input.
    fn execute(&mut self, input: ExecutionInput) -> Result<ExecutionReport, RuntimeError>;

    /// Terminates execution (if running).
    fn terminate(&mut self) -> Result<(), RuntimeError>;

    /// Returns the adapter configuration.
    fn config(&self) -> &AdapterConfig;
}

/// A mock adapter for testing.
#[derive(Debug)]
pub struct MockAdapter {
    config: AdapterConfig,
    prepared: bool,
    response: Option<ExecutionReport>,
}

impl MockAdapter {
    /// Creates a new mock adapter.
    pub fn new() -> Self {
        Self {
            config: AdapterConfig::new("mock", PathBuf::from("/bin/echo")),
            prepared: false,
            response: None,
        }
    }

    /// Sets the response to return on execute.
    #[must_use]
    pub fn with_response(mut self, report: ExecutionReport) -> Self {
        self.response = Some(report);
        self
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for MockAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prepare(&mut self, _task_id: Uuid, _worktree: &Path) -> Result<(), RuntimeError> {
        self.prepared = true;
        Ok(())
    }

    fn execute(&mut self, _input: ExecutionInput) -> Result<ExecutionReport, RuntimeError> {
        if !self.prepared {
            return Err(RuntimeError::new(
                "not_prepared",
                "Adapter not prepared",
                false,
            ));
        }

        Ok(self.response.clone().unwrap_or_else(|| {
            ExecutionReport::success(
                Duration::from_secs(1),
                "mock output".to_string(),
                String::new(),
            )
        }))
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        self.prepared = false;
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_input_creation() {
        let input = ExecutionInput {
            task_description: "Write a test".to_string(),
            success_criteria: "Test passes".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        assert!(input.prior_attempts.is_empty());
    }

    #[test]
    fn execution_report_success() {
        let report =
            ExecutionReport::success(Duration::from_secs(5), "output".to_string(), String::new());

        assert!(report.is_success());
        assert_eq!(report.exit_code, 0);
    }

    #[test]
    fn execution_report_failure() {
        let report = ExecutionReport::failure(
            1,
            Duration::from_secs(2),
            RuntimeError::new("test_error", "Test failed", false),
        );

        assert!(!report.is_success());
        assert_eq!(report.exit_code, 1);
    }

    #[test]
    fn adapter_config_builder() {
        let config = AdapterConfig::new("test", PathBuf::from("/bin/test"))
            .with_timeout(Duration::from_secs(60))
            .with_arg("--verbose")
            .with_env("DEBUG", "true");

        assert_eq!(config.name, "test");
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.args, vec!["--verbose"]);
        assert_eq!(config.env.get("DEBUG"), Some(&"true".to_string()));
    }

    #[test]
    fn parse_env_inherit_mode_rejects_invalid_values() {
        let mut overlay = HashMap::new();
        overlay.insert(
            ENV_POLICY_INHERIT_MODE_KEY.to_string(),
            "mystery".to_string(),
        );
        let error = parse_inherit_mode(&overlay).expect_err("invalid mode should fail");
        assert_eq!(error.code, "runtime_env_inherit_mode_invalid");
    }

    #[test]
    fn protected_runtime_env_none_mode_uses_only_overlay() {
        let mut overlay = HashMap::new();
        overlay.insert(
            ENV_POLICY_INHERIT_MODE_KEY.to_string(),
            ENV_POLICY_INHERIT_NONE.to_string(),
        );
        overlay.insert("PATH".to_string(), "/overlay/bin".to_string());
        overlay.insert("OPENROUTER_API_KEY".to_string(), "secret".to_string());

        let built = build_protected_runtime_environment(&overlay).expect("env build should work");
        assert_eq!(built.provenance.inherit_mode, RuntimeEnvInheritMode::None);
        assert!(built.provenance.inherited_keys.is_empty());
        assert_eq!(built.env.get("PATH"), Some(&"/overlay/bin".to_string()));
        assert_eq!(
            built.provenance.explicit_sensitive_overlay_keys,
            vec!["OPENROUTER_API_KEY".to_string()]
        );
        assert!(!built.env.contains_key(ENV_POLICY_INHERIT_MODE_KEY));
    }

    #[test]
    fn protected_runtime_env_filters_sensitive_and_reserved_inherited_vars() {
        let mut overlay = HashMap::new();
        overlay.insert(
            ENV_POLICY_INHERIT_MODE_KEY.to_string(),
            ENV_POLICY_INHERIT_ALL.to_string(),
        );

        let old_path = std::env::var("PATH").ok();
        let old_token = std::env::var("PARENT_TOKEN").ok();
        let old_reserved = std::env::var("HIVEMIND_TASK_ID").ok();
        std::env::set_var("PATH", "/tmp/hardened-path");
        std::env::set_var("PARENT_TOKEN", "super-secret");
        std::env::set_var("HIVEMIND_TASK_ID", "injected");

        let built = build_protected_runtime_environment(&overlay).expect("env build should work");

        match old_path {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
        match old_token {
            Some(value) => std::env::set_var("PARENT_TOKEN", value),
            None => std::env::remove_var("PARENT_TOKEN"),
        }
        match old_reserved {
            Some(value) => std::env::set_var("HIVEMIND_TASK_ID", value),
            None => std::env::remove_var("HIVEMIND_TASK_ID"),
        }

        assert_eq!(built.provenance.inherit_mode, RuntimeEnvInheritMode::All);
        assert!(built
            .provenance
            .dropped_sensitive_inherited_keys
            .iter()
            .any(|key| key == "PARENT_TOKEN"));
        assert!(built
            .provenance
            .dropped_reserved_inherited_keys
            .iter()
            .any(|key| key == "HIVEMIND_TASK_ID"));
        assert!(!built.env.contains_key("PARENT_TOKEN"));
        assert!(!built.env.contains_key("HIVEMIND_TASK_ID"));
        assert_eq!(
            built.env.get("PATH"),
            Some(&"/tmp/hardened-path".to_string())
        );
    }

    #[test]
    fn mock_adapter_lifecycle() {
        let mut adapter = MockAdapter::new();

        assert!(adapter.initialize().is_ok());

        let worktree = PathBuf::from("/tmp/test");
        let task_id = Uuid::new_v4();

        adapter.prepare(task_id, &worktree).unwrap();

        let input = ExecutionInput {
            task_description: "Test task".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        let report = adapter.execute(input).unwrap();
        assert!(report.is_success());

        adapter.terminate().unwrap();
    }

    #[test]
    fn mock_adapter_custom_response() {
        let custom_report = ExecutionReport::failure(
            1,
            Duration::from_secs(3),
            RuntimeError::new("custom", "Custom error", true),
        );

        let mut adapter = MockAdapter::new().with_response(custom_report);
        adapter
            .prepare(Uuid::new_v4(), &PathBuf::from("/tmp"))
            .unwrap();

        let input = ExecutionInput {
            task_description: "Test".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        let report = adapter.execute(input).unwrap();
        assert!(!report.is_success());
    }

    #[test]
    fn runtime_error_types() {
        let timeout = RuntimeError::timeout(Duration::from_secs(60));
        assert_eq!(timeout.code, "timeout");
        assert!(timeout.recoverable);

        let crash = RuntimeError::crash("Segmentation fault");
        assert_eq!(crash.code, "crash");
        assert!(!crash.recoverable);
    }
}
