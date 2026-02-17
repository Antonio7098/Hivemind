//! `OpenCode` adapter - wrapper for `OpenCode` CLI runtime.
//!
//! This adapter wraps the `OpenCode` CLI to provide task execution
//! capabilities within Hivemind's orchestration framework.

use super::runtime::{
    format_execution_prompt, AdapterConfig, ExecutionInput, ExecutionReport,
    InteractiveAdapterEvent, InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use signal_hook::consts::SIGINT;
use signal_hook::iterator::Signals;
use std::env;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use uuid::Uuid;

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn status_with_retry(
    binary: &std::path::Path,
    arg: &str,
) -> std::io::Result<std::process::ExitStatus> {
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..50 {
        match Command::new(binary)
            .arg(arg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
        {
            Ok(status) => return Ok(status),
            Err(e) if e.raw_os_error() == Some(26) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => return Err(e),
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("Executable remained busy")))
}

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
        let model = env::var("HIVEMIND_OPENCODE_MODEL").ok();
        Self {
            base: AdapterConfig::new("opencode", binary_path)
                .with_timeout(Duration::from_secs(600)), // 10 minutes
            model,
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
        format_execution_prompt(input)
    }

    #[allow(clippy::too_many_lines)]
    pub fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        mut on_event: F,
    ) -> Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        enum Msg {
            Output(String),
            Interrupt,
            Exit(portable_pty::ExitStatus),
            OutputDone,
        }

        let worktree = self
            .worktree
            .as_ref()
            .ok_or_else(|| RuntimeError::new("not_prepared", "Adapter not prepared", false))?;

        let start = Instant::now();
        let timeout = self.config.base.timeout;
        let formatted_input = self.format_input(input);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| {
                RuntimeError::new("pty_open_failed", format!("Failed to open PTY: {e}"), false)
            })?;

        let mut cmd = CommandBuilder::new(&self.config.base.binary_path);
        cmd.cwd(worktree);

        for (key, value) in &self.config.base.env {
            cmd.env(key, value);
        }

        let is_opencode_binary = self
            .config
            .base
            .binary_path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| {
                let lower = s.to_ascii_lowercase();
                lower.contains("opencode") || lower.contains("kilo")
            });

        if is_opencode_binary {
            cmd.arg("run");

            let has_model_flag = self
                .config
                .base
                .args
                .iter()
                .any(|a| a == "--model" || a == "-m" || a.starts_with("--model="));
            if !has_model_flag {
                if let Some(model) = &self.config.model {
                    cmd.arg("--model");
                    cmd.arg(model);
                }
            }

            if self.config.verbose {
                let has_print_logs = self.config.base.args.iter().any(|a| a == "--print-logs");
                if !has_print_logs {
                    cmd.arg("--print-logs");
                }
            }

            cmd.args(&self.config.base.args);
            cmd.arg(formatted_input.clone());
        } else if self.config.base.args.is_empty() {
            return Err(RuntimeError::new(
                "missing_args",
                "No runtime args configured; either point to the opencode binary or provide args",
                true,
            ));
        } else {
            cmd.args(&self.config.base.args);
        }

        let portable_pty::PtyPair { master, slave } = pair;

        let child = slave.spawn_command(cmd).map_err(|e| {
            RuntimeError::new(
                "spawn_failed",
                format!("Failed to spawn process: {e}"),
                false,
            )
        })?;

        // IMPORTANT: drop the parent's handle to the PTY slave.
        // If we keep it open, the master reader may never observe EOF/hangup after the child exits,
        // causing interactive sessions to hang indefinitely.
        drop(slave);

        let mut writer = master.take_writer().map_err(|e| {
            RuntimeError::new(
                "pty_writer_failed",
                format!("Failed to open PTY writer: {e}"),
                false,
            )
        })?;
        let mut reader = master.try_clone_reader().map_err(|e| {
            RuntimeError::new(
                "pty_reader_failed",
                format!("Failed to open PTY reader: {e}"),
                false,
            )
        })?;

        if !is_opencode_binary {
            let _ = writer.write_all(formatted_input.as_bytes());
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }

        let (tx, rx) = mpsc::channel::<Msg>();

        let output_tx = tx.clone();
        let output_handle = std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            loop {
                let Ok(n) = reader.read(&mut buf) else {
                    break;
                };
                if n == 0 {
                    break;
                }
                let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = output_tx.send(Msg::Output(chunk));
            }
            let _ = output_tx.send(Msg::OutputDone);
        });

        let mut killer = child.clone_killer();
        let wait_tx = tx.clone();
        let mut wait_child = child;
        let wait_handle = std::thread::spawn(move || {
            if let Ok(status) = wait_child.wait() {
                let _ = wait_tx.send(Msg::Exit(status));
            }
        });

        let mut signals = Signals::new([SIGINT]).map_err(|e| {
            RuntimeError::new(
                "signal_register_failed",
                format!("Failed to register SIGINT handler: {e}"),
                false,
            )
        })?;

        let _raw = RawModeGuard::new().map_err(|e| {
            RuntimeError::new(
                "interactive_tty_failed",
                format!("Failed to enable raw terminal mode: {e}"),
                false,
            )
        })?;

        let mut terminated_reason: Option<String> = None;
        let mut stdout = String::new();
        let mut exit_status: Option<portable_pty::ExitStatus> = None;
        let mut output_done = false;

        let mut input_line = String::new();
        let mut grace_deadline: Option<Instant> = None;

        loop {
            if terminated_reason.is_none() && start.elapsed() > timeout {
                terminated_reason = Some("timeout".to_string());
                grace_deadline = Some(Instant::now() + Duration::from_millis(200));
                let _ = writer.write_all(b"\x03");
                let _ = writer.flush();
            }

            for _sig in signals.pending() {
                if terminated_reason.is_none() {
                    let _ = tx.send(Msg::Interrupt);
                }
            }

            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Output(chunk) => {
                        stdout.push_str(&chunk);
                        on_event(InteractiveAdapterEvent::Output { content: chunk }).map_err(
                            |e| RuntimeError::new("interactive_callback_failed", e, false),
                        )?;
                    }
                    Msg::Interrupt => {
                        if terminated_reason.is_none() {
                            terminated_reason = Some("interrupted".to_string());
                            grace_deadline = Some(Instant::now() + Duration::from_millis(200));
                            on_event(InteractiveAdapterEvent::Interrupted).map_err(|e| {
                                RuntimeError::new("interactive_callback_failed", e, false)
                            })?;
                            let _ = writer.write_all(b"\x03");
                            let _ = writer.flush();
                        }
                    }
                    Msg::Exit(status) => {
                        exit_status = Some(status);
                    }
                    Msg::OutputDone => {
                        output_done = true;
                    }
                }
            }

            if let Some(deadline) = grace_deadline {
                if Instant::now() >= deadline {
                    let _ = killer.kill();
                    grace_deadline = None;
                }
            }

            if output_done && exit_status.is_some() {
                break;
            }

            if event::poll(Duration::from_millis(20)).map_err(|e| {
                RuntimeError::new(
                    "interactive_input_failed",
                    format!("Failed to poll input: {e}"),
                    false,
                )
            })? {
                let ev = event::read().map_err(|e| {
                    RuntimeError::new(
                        "interactive_input_failed",
                        format!("Failed to read input: {e}"),
                        false,
                    )
                })?;

                match ev {
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Char('c'),
                        modifiers,
                        ..
                    }) if modifiers.contains(KeyModifiers::CONTROL) => {
                        let _ = tx.send(Msg::Interrupt);
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Enter,
                        ..
                    }) => {
                        let line = std::mem::take(&mut input_line);
                        on_event(InteractiveAdapterEvent::Input {
                            content: line.clone(),
                        })
                        .map_err(|e| RuntimeError::new("interactive_callback_failed", e, false))?;
                        let _ = writer.write_all(b"\r");
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Backspace,
                        ..
                    }) => {
                        input_line.pop();
                        let _ = writer.write_all(&[0x7f]);
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Tab, ..
                    }) => {
                        input_line.push('\t');
                        let _ = writer.write_all(b"\t");
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Char(ch),
                        modifiers,
                        ..
                    }) if !modifiers.contains(KeyModifiers::CONTROL)
                        && !modifiers.contains(KeyModifiers::ALT) =>
                    {
                        input_line.push(ch);
                        let mut buf = [0u8; 4];
                        let s = ch.encode_utf8(&mut buf);
                        let _ = writer.write_all(s.as_bytes());
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Left,
                        ..
                    }) => {
                        let _ = writer.write_all(b"\x1b[D");
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Right,
                        ..
                    }) => {
                        let _ = writer.write_all(b"\x1b[C");
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Up, ..
                    }) => {
                        let _ = writer.write_all(b"\x1b[A");
                        let _ = writer.flush();
                    }
                    CrosstermEvent::Key(KeyEvent {
                        code: KeyCode::Down,
                        ..
                    }) => {
                        let _ = writer.write_all(b"\x1b[B");
                        let _ = writer.flush();
                    }
                    _ => {}
                }
            }
        }

        let _ = output_handle.join();
        let _ = wait_handle.join();

        let exit_code = exit_status
            .as_ref()
            .map_or(-1, |s| i32::try_from(s.exit_code()).unwrap_or(-1));
        let duration = start.elapsed();

        let report = if exit_code == 0 {
            ExecutionReport::success(duration, stdout, String::new())
        } else {
            ExecutionReport {
                exit_code,
                duration,
                stdout,
                stderr: String::new(),
                files_created: Vec::new(),
                files_modified: Vec::new(),
                files_deleted: Vec::new(),
                errors: vec![RuntimeError::new(
                    "nonzero_exit",
                    format!("Process exited with code {exit_code}"),
                    true,
                )],
            }
        };

        Ok(InteractiveExecutionResult {
            report,
            terminated_reason,
        })
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
        let result = status_with_retry(binary, "--version");

        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => {
                // Try --help as fallback
                let help_result = status_with_retry(binary, "--help");

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

        let formatted_input = self.format_input(&input);

        let scope_trace_log = self
            .config
            .base
            .env
            .get("HIVEMIND_SCOPE_TRACE_LOG")
            .filter(|v| !v.trim().is_empty())
            .cloned();
        let trace_enabled = scope_trace_log.as_ref().is_some_and(|_| {
            Command::new("strace")
                .args([
                    "-o",
                    "/dev/null",
                    "-e",
                    "trace=file",
                    "--",
                    "/usr/bin/env",
                    "true",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        });

        // Build and spawn the command
        let mut cmd = if trace_enabled {
            let mut wrapped = Command::new("strace");
            wrapped
                .arg("-f")
                .arg("-qq")
                .arg("-e")
                .arg("trace=file")
                .arg("-o")
                .arg(scope_trace_log.expect("trace path"))
                .arg("--")
                .arg(&self.config.base.binary_path);
            wrapped
        } else {
            Command::new(&self.config.base.binary_path)
        };
        cmd.current_dir(worktree)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let is_opencode_binary = self
            .config
            .base
            .binary_path
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| {
                let lower = s.to_ascii_lowercase();
                lower.contains("opencode") || lower.contains("kilo")
            });

        if is_opencode_binary {
            cmd.stdin(Stdio::null());
            cmd.arg("run");

            let has_model_flag = self
                .config
                .base
                .args
                .iter()
                .any(|a| a == "--model" || a == "-m" || a.starts_with("--model="));
            if !has_model_flag {
                if let Some(model) = &self.config.model {
                    cmd.arg("--model").arg(model);
                }
            }

            if self.config.verbose {
                let has_print_logs = self.config.base.args.iter().any(|a| a == "--print-logs");
                if !has_print_logs {
                    cmd.arg("--print-logs");
                }
            }

            cmd.args(&self.config.base.args);
            cmd.arg(formatted_input.clone());
        } else if self.config.base.args.is_empty() {
            return Err(RuntimeError::new(
                "missing_args",
                "No runtime args configured; either point to the opencode binary or provide args",
                true,
            ));
        } else {
            cmd.stdin(Stdio::piped());
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
            if let Err(e) = stdin.write_all(formatted_input.as_bytes()) {
                // Some runtimes may exit quickly (e.g. error paths); in that case stdin can be
                // closed before we finish writing. Treat EPIPE as non-fatal and continue to
                // collect exit status and output.
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    return Err(RuntimeError::new(
                        "stdin_write_failed",
                        format!("Failed to write to stdin: {e}"),
                        true,
                    ));
                }
            }
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
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                self.process = None;
                return Err(RuntimeError::timeout(timeout));
            };

            if start.elapsed() > timeout {
                let _ = stdout_handle.join();
                let _ = stderr_handle.join();
                self.process = None;
                return Err(RuntimeError::timeout(timeout));
            }

            if let Some(status) = process.try_wait().map_err(|e| {
                RuntimeError::new(
                    "wait_failed",
                    format!("Failed to wait on process: {e}"),
                    false,
                )
            })? {
                break status;
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
            Ok(ExecutionReport::failure_with_output(
                exit_code,
                duration,
                RuntimeError::new(
                    "nonzero_exit",
                    format!("Process exited with code {exit_code}"),
                    true,
                ),
                stdout_content,
                stderr_content,
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
