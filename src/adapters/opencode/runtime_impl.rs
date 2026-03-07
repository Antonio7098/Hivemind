use super::*;

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
        cmd.env_clear();
        for (key, value) in deterministic_env_pairs(&self.config.base.env) {
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
