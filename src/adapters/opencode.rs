//! `OpenCode` adapter - wrapper for `OpenCode` CLI runtime.
//!
//! This adapter wraps the `OpenCode` CLI to provide task execution
//! capabilities within Hivemind's orchestration framework.

use super::runtime::{
    AdapterConfig, ExecutionInput, ExecutionReport, RuntimeAdapter, RuntimeError,
};
use std::fmt::Write as FmtWrite;
use std::io::{BufRead, BufReader, Write};
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
                let _ = writeln!(
                    prompt,
                    "- Attempt {attempt_number}: {summary}",
                );
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

    fn health_check(&self) -> Result<(), RuntimeError> {
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

    fn prepare(&mut self, worktree: &PathBuf, task_id: Uuid) -> Result<(), RuntimeError> {
        // Verify worktree exists
        if !worktree.exists() {
            return Err(RuntimeError::new(
                "worktree_not_found",
                format!("Worktree does not exist: {}", worktree.display()),
                false,
            ));
        }

        self.worktree = Some(worktree.clone());
        self.task_id = Some(task_id);
        Ok(())
    }

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

        // Wait with timeout
        let mut stdout_content = String::new();
        let mut stderr_content = String::new();

        if let Some(ref mut process) = self.process {
            // Read output in a loop with timeout checking
            let stdout = process.stdout.take();
            let stderr = process.stderr.take();

            if let Some(stdout) = stdout {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    if start.elapsed() > timeout {
                        let _ = process.kill();
                        return Err(RuntimeError::timeout(timeout));
                    }
                    if let Ok(line) = line {
                        stdout_content.push_str(&line);
                        stdout_content.push('\n');
                    }
                }
            }

            if let Some(stderr) = stderr {
                let reader = BufReader::new(stderr);
                for line in reader.lines().flatten() {
                    stderr_content.push_str(&line);
                    stderr_content.push('\n');
                }
            }

            // Wait for process to complete
            let status = process.wait().map_err(|e| {
                RuntimeError::new(
                    "wait_failed",
                    format!("Failed to wait for process: {e}"),
                    true,
                )
            })?;

            let duration = start.elapsed();
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
        } else {
            Err(RuntimeError::new(
                "no_process",
                "No process to wait on",
                false,
            ))
        }
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        if let Some(ref mut process) = self.process {
            process.kill().map_err(|e| {
                RuntimeError::new(
                    "kill_failed",
                    format!("Failed to kill process: {e}"),
                    false,
                )
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

        let result = adapter.prepare(&PathBuf::from("/nonexistent/path"), task_id);
        assert!(result.is_err());
    }

    #[test]
    fn prepare_with_valid_worktree() {
        let mut adapter = OpenCodeAdapter::with_defaults();
        let task_id = Uuid::new_v4();

        // Use /tmp which should exist
        let result = adapter.prepare(&PathBuf::from("/tmp"), task_id);
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

    // Note: Full execution tests require the actual opencode binary
    // and are better suited for integration tests
}
