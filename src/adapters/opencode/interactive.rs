use super::*;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use signal_hook::consts::SIGINT;
use signal_hook::iterator::Signals;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
mod raw_mode;
use raw_mode::*;

impl OpenCodeAdapter {
    pub(super) fn no_progress_timeout(&self) -> Option<Duration> {
        let timeout = self.config.base.timeout;
        if timeout.is_zero() {
            return None;
        }

        if let Some(raw) = self
            .config
            .base
            .env
            .get("HIVEMIND_RUNTIME_NO_PROGRESS_TIMEOUT_MS")
            .map(String::as_str)
        {
            let trimmed = raw.trim();
            if trimmed == "0" {
                return None;
            }
            if let Ok(ms) = trimmed.parse::<u64>() {
                return Some(Duration::from_millis(ms));
            }
        }

        Some(std::cmp::min(
            Duration::from_secs(45),
            std::cmp::max(Duration::from_secs(5), timeout / 4),
        ))
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
            FilesystemProgress {
                created: Vec<PathBuf>,
                modified: Vec<PathBuf>,
                deleted: Vec<PathBuf>,
            },
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
        let no_progress_timeout = self.no_progress_timeout();
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
        cmd.env_clear();
        for (key, value) in deterministic_env_pairs(&self.config.base.env) {
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

        let filesystem_tx = tx.clone();
        let filesystem_worktree = worktree.clone();
        let stop_filesystem_poll = Arc::new(AtomicBool::new(false));
        let filesystem_stop_flag = Arc::clone(&stop_filesystem_poll);
        let filesystem_handle = std::thread::spawn(move || {
            let mut previous = OpenCodeAdapter::capture_worktree_state(&filesystem_worktree);
            while !filesystem_stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(1));
                let current = OpenCodeAdapter::capture_worktree_state(&filesystem_worktree);
                let delta = OpenCodeAdapter::diff_worktree_state(&previous, &current);
                if !delta.created.is_empty()
                    || !delta.modified.is_empty()
                    || !delta.deleted.is_empty()
                {
                    let _ = filesystem_tx.send(Msg::FilesystemProgress {
                        created: delta.created,
                        modified: delta.modified,
                        deleted: delta.deleted,
                    });
                }
                previous = current;
            }
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
        let mut last_progress_at = Instant::now();

        let mut input_line = String::new();
        let mut grace_deadline: Option<Instant> = None;

        loop {
            if terminated_reason.is_none() && start.elapsed() > timeout {
                terminated_reason = Some("timeout".to_string());
                grace_deadline = Some(Instant::now() + Duration::from_millis(200));
                let _ = writer.write_all(b"\x03");
                let _ = writer.flush();
            }
            if terminated_reason.is_none()
                && no_progress_timeout.is_some_and(|limit| last_progress_at.elapsed() > limit)
            {
                terminated_reason = Some("no_observable_progress_timeout".to_string());
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
                        last_progress_at = Instant::now();
                        stdout.push_str(&chunk);
                        on_event(InteractiveAdapterEvent::Output { content: chunk }).map_err(
                            |e| RuntimeError::new("interactive_callback_failed", e, false),
                        )?;
                    }
                    Msg::FilesystemProgress {
                        created,
                        modified,
                        deleted,
                    } => {
                        last_progress_at = Instant::now();
                        on_event(InteractiveAdapterEvent::FilesystemObserved {
                            files_created: created,
                            files_modified: modified,
                            files_deleted: deleted,
                        })
                        .map_err(|e| RuntimeError::new("interactive_callback_failed", e, false))?;
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

        stop_filesystem_poll.store(true, Ordering::Relaxed);
        let _ = output_handle.join();
        let _ = filesystem_handle.join();
        let _ = wait_handle.join();

        let exit_code = exit_status
            .as_ref()
            .map_or(-1, |s| i32::try_from(s.exit_code()).unwrap_or(-1));
        let duration = start.elapsed();

        let report = if let Some(reason) = terminated_reason.as_deref() {
            let failure_exit_code = if exit_code == 0 { -1 } else { exit_code };
            let error = match reason {
                "timeout" => RuntimeError::timeout(duration),
                "no_observable_progress_timeout" => RuntimeError::new(
                    "no_observable_progress_timeout",
                    format!(
                        "Runtime produced no observable progress for {:?}",
                        no_progress_timeout.unwrap_or(duration)
                    ),
                    true,
                ),
                "interrupted" => {
                    RuntimeError::new("interrupted", "Runtime execution was interrupted", true)
                }
                other => RuntimeError::new(other, format!("Runtime terminated: {other}"), true),
            };
            ExecutionReport::failure_with_output(
                failure_exit_code,
                duration,
                error,
                stdout,
                String::new(),
            )
        } else if exit_code == 0 {
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
                native_invocation: None,
                structured_runtime_observations: Vec::new(),
            }
        };

        Ok(InteractiveExecutionResult {
            report,
            terminated_reason,
        })
    }
}
