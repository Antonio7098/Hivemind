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
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

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
