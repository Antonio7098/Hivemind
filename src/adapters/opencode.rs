//! `OpenCode` adapter - wrapper for `OpenCode` CLI runtime.
//!
//! This adapter wraps the `OpenCode` CLI to provide task execution
//! capabilities within Hivemind's orchestration framework.

use super::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, RuntimeAdapter, RuntimeError,
};
use std::fmt::Write as FmtWrite;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

/// `OpenCode` adapter configuration.
#[derive(Debug, Clone)]
pub struct OpenCodeConfig {
    /// Base adapter config.
    pub base: AdapterConfig,
    /// Model to use (if configurable).
    pub model: Option<String>,
    /// Whether to enable verbose output.
    pub verbose: bool,
}

impl OpenCodeConfig {
    /// Creates a new `OpenCode` config with default settings.
    pub fn new(binary_path: PathBuf) -> Self {
        Self {
            base: AdapterConfig::new("opencode", binary_path)
                .with_timeout(Duration::from_secs(600)), // 10 minutes
            model: None,
            verbose: false,
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Enables verbose mode.
    #[must_use]
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Sets the timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.base.timeout = timeout;
        self
    }
}

impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self::new(PathBuf::from("opencode"))
    }
}

/// `OpenCode` runtime adapter.
pub struct OpenCodeAdapter {
    config: OpenCodeConfig,
    worktree: Option<PathBuf>,
    task_id: Option<Uuid>,
    process: Option<Child>,
}

impl OpenCodeAdapter {
    /// Creates a new `OpenCode` adapter.
    pub fn new(config: OpenCodeConfig) -> Self {
        Self {
            config,
            worktree: None,
            task_id: None,
            process: None,
        }
    }

    /// Creates an adapter with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OpenCodeConfig::default())
    }

    /// Formats the input for the runtime.
    #[allow(clippy::unused_self)]
    fn format_input(&self, input: &ExecutionInput) -> String {
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
                let _ = writeln!(prompt, "- Attempt {attempt_number}: {summary}",);
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
}

impl RuntimeAdapter for OpenCodeAdapter {
    fn name(&self) -> &str {
        &self.config.base.name
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        // Check if binary exists and is executable
        let binary = &self.config.base.binary_path;

        // Try to run with --version or --help
        let result = Command::new(binary)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => {
                // Try --help as fallback
                let help_result = Command::new(binary)
                    .arg("--help")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();

                match help_result {
                    Ok(status) if status.success() => Ok(()),
                    _ => Err(RuntimeError::new(
                        "health_check_failed",
                        format!("Binary {} is not responding correctly", binary.display()),
                        false,
                    )),
                }
            }
            Err(e) => Err(RuntimeError::new(
                "binary_not_found",
                format!("Cannot execute {}: {e}", binary.display()),
                false,
            )),
        }
    }

    fn prepare(&mut self, task_id: Uuid, worktree: &std::path::Path) -> Result<(), RuntimeError> {
        // Verify worktree exists
        if !worktree.exists() {
            return Err(RuntimeError::new(
                "worktree_not_found",
                format!("Worktree does not exist: {}", worktree.display()),
                false,
            ));
        }

        self.worktree = Some(worktree.to_path_buf());
        self.task_id = Some(task_id);
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn execute(&mut self, input: ExecutionInput) -> Result<ExecutionReport, RuntimeError> {
        let worktree = self
            .worktree
            .as_ref()
            .ok_or_else(|| RuntimeError::new("not_prepared", "Adapter not prepared", false))?;

        let start = Instant::now();
        let timeout = self.config.base.timeout;

        // Build and spawn the command
        let mut cmd = Command::new(&self.config.base.binary_path);
        cmd.current_dir(worktree)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !self.config.base.args.is_empty() {
            cmd.args(&self.config.base.args);
        }

        // Add environment variables
        for (key, value) in &self.config.base.env {
            cmd.env(key, value);
        }

        // Spawn process
        let mut child = cmd.spawn().map_err(|e| {
            RuntimeError::new(
                "spawn_failed",
                format!("Failed to spawn process: {e}"),
                false,
            )
        })?;

        // Write input to stdin
        if let Some(ref mut stdin) = child.stdin {
            let formatted_input = self.format_input(&input);
            stdin.write_all(formatted_input.as_bytes()).map_err(|e| {
                RuntimeError::new(
                    "stdin_write_failed",
                    format!("Failed to write to stdin: {e}"),
                    true,
                )
            })?;
        }
        // Close stdin
        drop(child.stdin.take());

        self.process = Some(child);

        let (stdout_handle, stderr_handle) = if let Some(ref mut process) = self.process {
            let stdout = process.stdout.take().ok_or_else(|| {
                RuntimeError::new("stdout_capture_failed", "Missing stdout pipe", false)
            })?;
            let stderr = process.stderr.take().ok_or_else(|| {
                RuntimeError::new("stderr_capture_failed", "Missing stderr pipe", false)
            })?;

            let stdout_handle = std::thread::spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut out = String::new();
                let _ = reader.read_to_string(&mut out);
                out
            });
            let stderr_handle = std::thread::spawn(move || {
                let mut reader = BufReader::new(stderr);
                let mut out = String::new();
                let _ = reader.read_to_string(&mut out);
                out
            });

            (stdout_handle, stderr_handle)
        } else {
            return Err(RuntimeError::new(
                "no_process",
                "No process to wait on",
                false,
            ));
        };

        let status = loop {
            let Some(ref mut process) = self.process else {
                return Err(RuntimeError::new(
                    "no_process",
                    "No process to wait on",
                    false,
                ));
            };

            if let Some(status) = process.try_wait().map_err(|e| {
                RuntimeError::new(
                    "wait_failed",
                    format!("Failed to wait for process: {e}"),
                    true,
                )
            })? {
                break status;
            }

            if start.elapsed() > timeout {
                let _ = process.kill();
                let _ = process.wait();
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                self.process = None;
                return Err(RuntimeError::timeout(timeout));
            }

            std::thread::sleep(Duration::from_millis(10));
        };

        let duration = start.elapsed();
        let stdout_content = stdout_handle.join().unwrap_or_else(|_| String::new());
        let stderr_content = stderr_handle.join().unwrap_or_else(|_| String::new());

        self.process = None;

        let exit_code = status.code().unwrap_or(-1);
        if exit_code == 0 {
            Ok(ExecutionReport::success(
                duration,
                stdout_content,
                stderr_content,
            ))
        } else {
            Ok(ExecutionReport::failure(
                exit_code,
                duration,
                RuntimeError::new(
                    "nonzero_exit",
                    format!("Process exited with code {exit_code}"),
                    true,
                ),
            ))
        }
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        if let Some(ref mut process) = self.process {
            process.kill().map_err(|e| {
                RuntimeError::new("kill_failed", format!("Failed to kill process: {e}"), false)
            })?;
        }

        self.process = None;
        self.worktree = None;
        self.task_id = None;
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn opencode_config_creation() {
        let config = OpenCodeConfig::new(PathBuf::from("/usr/bin/opencode"))
            .with_model("gpt-4")
            .with_verbose(true)
            .with_timeout(Duration::from_secs(120));

        assert_eq!(config.model, Some("gpt-4".to_string()));
        assert!(config.verbose);
        assert_eq!(config.base.timeout, Duration::from_secs(120));
    }

    #[test]
    fn opencode_config_default() {
        let config = OpenCodeConfig::default();
        assert_eq!(config.base.name, "opencode");
        assert!(config.model.is_none());
        assert!(!config.verbose);
    }

    #[test]
    fn adapter_creation() {
        let adapter = OpenCodeAdapter::with_defaults();
        assert_eq!(adapter.name(), "opencode");
    }

    #[test]
    fn input_formatting() {
        let adapter = OpenCodeAdapter::with_defaults();

        let input = ExecutionInput {
            task_description: "Write a function".to_string(),
            success_criteria: "Function works".to_string(),
            context: Some("This is for testing".to_string()),
            prior_attempts: vec![],
            verifier_feedback: None,
        };

        let formatted = adapter.format_input(&input);
        assert!(formatted.contains("Write a function"));
        assert!(formatted.contains("Function works"));
        assert!(formatted.contains("This is for testing"));
    }

    #[test]
    fn input_formatting_with_retries() {
        let adapter = OpenCodeAdapter::with_defaults();

        let input = ExecutionInput {
            task_description: "Fix the bug".to_string(),
            success_criteria: "Tests pass".to_string(),
            context: None,
            prior_attempts: vec![super::super::runtime::AttemptSummary {
                attempt_number: 1,
                summary: "Tried approach A".to_string(),
                failure_reason: Some("Tests failed".to_string()),
            }],
            verifier_feedback: Some("Check edge cases".to_string()),
        };

        let formatted = adapter.format_input(&input);
        assert!(formatted.contains("Attempt 1"));
        assert!(formatted.contains("Tried approach A"));
        assert!(formatted.contains("Check edge cases"));
    }

    #[test]
    fn prepare_requires_existing_worktree() {
        let mut adapter = OpenCodeAdapter::with_defaults();
        let task_id = Uuid::new_v4();

        let result = adapter.prepare(task_id, &PathBuf::from("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn prepare_with_valid_worktree() {
        let mut adapter = OpenCodeAdapter::with_defaults();
        let task_id = Uuid::new_v4();

        // Use /tmp which should exist
        let result = adapter.prepare(task_id, &PathBuf::from("/tmp"));
        assert!(result.is_ok());
        assert!(adapter.worktree.is_some());
        assert!(adapter.task_id.is_some());
    }

    #[test]
    fn terminate_clears_state() {
        let mut adapter = OpenCodeAdapter::with_defaults();
        adapter.worktree = Some(PathBuf::from("/tmp"));
        adapter.task_id = Some(Uuid::new_v4());

        adapter.terminate().unwrap();

        assert!(adapter.worktree.is_none());
        assert!(adapter.task_id.is_none());
    }

    #[test]
    fn execute_enforces_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
        cfg.base.args = vec!["sh".to_string(), "-c".to_string(), "sleep 2".to_string()];
        cfg.base.timeout = Duration::from_millis(50);

        let mut adapter = OpenCodeAdapter::new(cfg);
        adapter.initialize().unwrap();
        adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

        let input = ExecutionInput {
            task_description: "Test".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        let err = adapter.execute(input).unwrap_err();
        assert_eq!(err.code, "timeout");
    }

    #[test]
    fn initialize_falls_back_to_help_when_version_fails() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let script_path = tmp.path().join("fake_runtime.sh");
        std::fs::write(
            &script_path,
            "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ]; then exit 1; fi\nif [ \"$1\" = \"--help\" ]; then exit 0; fi\nexit 0\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms).unwrap();

        let cfg = OpenCodeConfig::new(script_path);
        let mut adapter = OpenCodeAdapter::new(cfg);
        adapter.initialize().unwrap();
    }

    #[test]
    fn execute_success_captures_stdout_and_stderr() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
        cfg.base.args = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo ok_stdout; echo ok_stderr 1>&2".to_string(),
        ];
        cfg.base.timeout = Duration::from_secs(1);

        let mut adapter = OpenCodeAdapter::new(cfg);
        adapter.initialize().unwrap();
        adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

        let input = ExecutionInput {
            task_description: "Test".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        let report = adapter.execute(input).unwrap();
        assert_eq!(report.exit_code, 0);
        assert!(report.stdout.contains("ok_stdout"));
        assert!(report.stderr.contains("ok_stderr"));
    }

    #[test]
    fn execute_nonzero_exit_returns_failure_report() {
        let tmp = tempfile::tempdir().expect("tempdir");

        let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
        cfg.base.args = vec![
            "sh".to_string(),
            "-c".to_string(),
            "echo bad; exit 7".to_string(),
        ];
        cfg.base.timeout = Duration::from_secs(1);

        let mut adapter = OpenCodeAdapter::new(cfg);
        adapter.initialize().unwrap();
        adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

        let input = ExecutionInput {
            task_description: "Test".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
        };

        let report = adapter.execute(input).unwrap();
        assert_eq!(report.exit_code, 7);
        assert!(report.errors.iter().any(|e| e.code == "nonzero_exit"));
    }

    // Note: Full execution tests require the actual opencode binary
    // and are better suited for integration tests
}
