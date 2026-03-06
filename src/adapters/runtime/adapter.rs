use super::NativeInvocationTrace;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub exit_code: i32,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
    pub files_created: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub files_deleted: Vec<PathBuf>,
    pub errors: Vec<RuntimeError>,
    #[serde(default)]
    pub native_invocation: Option<NativeInvocationTrace>,
}

impl ExecutionReport {
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

    pub fn failure(exit_code: i32, duration: Duration, error: RuntimeError) -> Self {
        Self::failure_with_output(exit_code, duration, error, String::new(), String::new())
    }

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

    #[must_use]
    pub fn is_success(&self) -> bool {
        self.exit_code == 0 && self.errors.is_empty()
    }

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

    #[must_use]
    pub fn with_native_invocation(mut self, invocation: NativeInvocationTrace) -> Self {
        self.native_invocation = Some(invocation);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractiveAdapterEvent {
    Output { content: String },
    Input { content: String },
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveExecutionResult {
    pub report: ExecutionReport,
    pub terminated_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl RuntimeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }

    pub fn timeout(duration: Duration) -> Self {
        Self::new(
            "timeout",
            format!("Execution timed out after {duration:?}"),
            true,
        )
    }

    pub fn crash(message: impl Into<String>) -> Self {
        Self::new("crash", message, false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub name: String,
    pub binary_path: PathBuf,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub timeout: Duration,
    pub working_dir: Option<PathBuf>,
}

impl AdapterConfig {
    pub fn new(name: impl Into<String>, binary_path: PathBuf) -> Self {
        Self {
            name: name.into(),
            binary_path,
            args: Vec::new(),
            env: HashMap::new(),
            timeout: Duration::from_secs(300),
            working_dir: None,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }
}

pub trait RuntimeAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn initialize(&mut self) -> Result<(), RuntimeError>;
    fn prepare(&mut self, task_id: Uuid, worktree: &Path) -> Result<(), RuntimeError>;
    fn execute(&mut self, input: super::ExecutionInput) -> Result<ExecutionReport, RuntimeError>;
    fn terminate(&mut self) -> Result<(), RuntimeError>;
    fn config(&self) -> &AdapterConfig;
}
