//! Runtime adapter interface for execution backends.
//!
//! Runtime adapters are the only components that interact directly with
//! execution runtimes (e.g. Claude Code, `OpenCode`, Codex CLI).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
        Self {
            exit_code,
            duration,
            stdout: String::new(),
            stderr: String::new(),
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

/// Runtime adapter lifecycle events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AdapterEvent {
    /// Runtime started.
    Started {
        adapter_name: String,
        task_id: Uuid,
        attempt_id: Uuid,
    },
    /// Output chunk received.
    OutputChunk {
        attempt_id: Uuid,
        stream: OutputStream,
        content: String,
    },
    /// Runtime exited.
    Exited {
        attempt_id: Uuid,
        exit_code: i32,
        duration: Duration,
    },
    /// Runtime was terminated (timeout or manual).
    Terminated { attempt_id: Uuid, reason: String },
    /// Error occurred.
    Error {
        attempt_id: Uuid,
        error: RuntimeError,
    },
}

/// Output stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// Trait for runtime adapters.
///
/// All adapters must implement this trait to be usable by Hivemind.
pub trait RuntimeAdapter: Send + Sync {
    /// Returns the adapter name.
    fn name(&self) -> &str;

    /// Checks if the adapter is healthy (binary exists, etc).
    fn health_check(&self) -> Result<(), RuntimeError>;

    /// Prepares the adapter for execution.
    fn prepare(&mut self, worktree: &std::path::Path, task_id: Uuid) -> Result<(), RuntimeError>;

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

    fn health_check(&self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prepare(&mut self, _worktree: &std::path::Path, _task_id: Uuid) -> Result<(), RuntimeError> {
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

        assert!(adapter.health_check().is_ok());

        let worktree = PathBuf::from("/tmp/test");
        let task_id = Uuid::new_v4();

        adapter.prepare(&worktree, task_id).unwrap();

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
            .prepare(&PathBuf::from("/tmp"), Uuid::new_v4())
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
    fn adapter_event_serialization() {
        let event = AdapterEvent::Started {
            adapter_name: "test".to_string(),
            task_id: Uuid::new_v4(),
            attempt_id: Uuid::new_v4(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("started"));
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
