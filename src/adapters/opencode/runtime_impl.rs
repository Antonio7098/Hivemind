use super::*;
use crate::adapters::json_output::{transform_json_output, JsonAdapterKind};
use crate::adapters::runtime::StructuredRuntimeObservation;
use crate::core::events::RuntimeOutputStream;
use crate::core::worktree::{WorktreeConfig, WorktreeManager};
use serde_json::Value;
use std::collections::HashSet;
use std::io::BufRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliCommandFlavor {
    OpenCodeFamily,
    Codex,
    Generic,
}

#[derive(Debug)]
struct LiveJsonRuntimeTracker {
    flavor: CliCommandFlavor,
    adapter_name: String,
    worktree: PathBuf,
    attempt_id: Option<String>,
    task_id: Option<String>,
    current_session_id: Option<String>,
    seen_session_ids: HashSet<String>,
    next_turn_ordinal: u32,
    observations: Vec<StructuredRuntimeObservation>,
    warnings: Vec<String>,
}

impl LiveJsonRuntimeTracker {
    fn new(
        flavor: CliCommandFlavor,
        adapter_name: String,
        worktree: PathBuf,
        env: &std::collections::HashMap<String, String>,
    ) -> Option<Self> {
        matches!(
            flavor,
            CliCommandFlavor::OpenCodeFamily | CliCommandFlavor::Codex
        )
        .then(|| Self {
            flavor,
            adapter_name,
            worktree,
            attempt_id: env.get("HIVEMIND_ATTEMPT_ID").cloned(),
            task_id: env.get("HIVEMIND_TASK_ID").cloned(),
            current_session_id: None,
            seen_session_ids: HashSet::new(),
            next_turn_ordinal: 0,
            observations: Vec::new(),
            warnings: Vec::new(),
        })
    }

    fn observe_stdout_line(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            return;
        };

        match self.flavor {
            CliCommandFlavor::OpenCodeFamily => self.observe_opencode_value(&value),
            CliCommandFlavor::Codex => self.observe_codex_value(&value),
            CliCommandFlavor::Generic => {}
        }
    }

    fn finish(self) -> (Vec<StructuredRuntimeObservation>, Vec<String>) {
        (self.observations, self.warnings)
    }

    fn observe_opencode_value(&mut self, value: &Value) {
        if let Some(session_id) = json_nested_str(value, &["sessionID"])
            .or_else(|| json_nested_str(value, &["sessionId"]))
        {
            self.note_session(session_id.to_string());
        }

        if json_nested_str(value, &["type"]) == Some("step_finish") {
            let provider_turn_id = json_nested_str(value, &["snapshot"]).map(ToString::to_string);
            self.record_turn_completed(provider_turn_id, None);
        }
    }

    fn observe_codex_value(&mut self, value: &Value) {
        match json_nested_str(value, &["type"]) {
            Some("thread.started") => {
                if let Some(thread_id) = json_nested_str(value, &["thread_id"]) {
                    self.note_session(thread_id.to_string());
                }
            }
            Some("turn.completed") => self.record_turn_completed(None, None),
            _ => {}
        }
    }

    fn note_session(&mut self, session_id: String) {
        self.current_session_id = Some(session_id.clone());
        if self.seen_session_ids.insert(session_id.clone()) {
            self.observations
                .push(StructuredRuntimeObservation::SessionObserved {
                    stream: RuntimeOutputStream::Stdout,
                    adapter_name: self.adapter_name.clone(),
                    session_id,
                });
        }
    }

    fn record_turn_completed(&mut self, provider_turn_id: Option<String>, summary: Option<String>) {
        self.next_turn_ordinal += 1;
        let ordinal = self.next_turn_ordinal;
        let (git_ref, commit_sha) = self.create_turn_ref(ordinal);
        self.observations
            .push(StructuredRuntimeObservation::TurnCompleted {
                stream: RuntimeOutputStream::Stdout,
                adapter_name: self.adapter_name.clone(),
                ordinal,
                provider_session_id: self.current_session_id.clone(),
                provider_turn_id,
                git_ref,
                commit_sha,
                summary,
            });
    }

    fn create_turn_ref(&mut self, ordinal: u32) -> (Option<String>, Option<String>) {
        let Some(attempt_id) = self.attempt_id.as_ref() else {
            return (None, None);
        };
        let task_segment = self
            .task_id
            .as_deref()
            .unwrap_or("unknown-task")
            .replace('/', "_");
        let ref_name =
            format!("refs/hivemind/transient/turns/{task_segment}/{attempt_id}/turn-{ordinal:04}");

        let manager = match WorktreeManager::new(self.worktree.clone(), WorktreeConfig::default()) {
            Ok(manager) => manager,
            Err(error) => {
                self.warnings.push(format!(
                    "turn ref initialization failed for turn {ordinal}: {error}"
                ));
                return (None, None);
            }
        };

        let message = format!("hivemind transient turn snapshot {ordinal}");
        match manager.create_hidden_snapshot_ref(&self.worktree, &ref_name, &message) {
            Ok(commit_sha) => (Some(ref_name), Some(commit_sha)),
            Err(error) => {
                self.warnings.push(format!(
                    "turn ref snapshot failed for turn {ordinal}: {error}"
                ));
                (None, None)
            }
        }
    }
}

fn json_nested_str<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn read_stream_to_string(
    stream: impl Read,
    mut tracker: Option<LiveJsonRuntimeTracker>,
) -> (String, Option<LiveJsonRuntimeTracker>) {
    let mut reader = BufReader::new(stream);
    let mut out = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if let Some(tracker) = tracker.as_mut() {
                    tracker.observe_stdout_line(&line);
                }
                out.push_str(&line);
            }
        }
    }
    (out, tracker)
}

fn command_flavor(binary: &std::path::Path) -> CliCommandFlavor {
    let Some(file_name) = binary.file_name().and_then(|value| value.to_str()) else {
        return CliCommandFlavor::Generic;
    };

    let lower = file_name.to_ascii_lowercase();
    if lower.contains("opencode") || lower.contains("kilo") {
        CliCommandFlavor::OpenCodeFamily
    } else if lower.contains("codex") {
        CliCommandFlavor::Codex
    } else {
        CliCommandFlavor::Generic
    }
}

fn strip_opencode_wrapper_args(args: &[String]) -> Vec<String> {
    let mut filtered = Vec::new();
    let mut skip_next = false;

    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "run" {
            continue;
        }
        if arg == "--format" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--format=") {
            continue;
        }
        filtered.push(arg.clone());
    }

    filtered
}

fn strip_codex_wrapper_args(args: &[String]) -> (Vec<String>, bool) {
    let mut filtered = Vec::new();
    let mut has_prompt_source = false;

    for arg in args {
        match arg.as_str() {
            "exec" | "--json" => {}
            "-" | "--prompt" | "-p" => {
                has_prompt_source = true;
                filtered.push(arg.clone());
            }
            _ => filtered.push(arg.clone()),
        }
    }

    (filtered, has_prompt_source)
}

fn has_model_flag(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--model" || arg == "-m" || arg.starts_with("--model="))
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

        let flavor = command_flavor(&self.config.base.binary_path);

        if flavor == CliCommandFlavor::OpenCodeFamily {
            let wrapper_args = strip_opencode_wrapper_args(&self.config.base.args);
            cmd.stdin(Stdio::null());
            cmd.arg("run");
            cmd.arg("--format").arg("json");

            if let Some(session_id) = &self.config.base.resume_session_id {
                cmd.arg("--session").arg(session_id);
            }

            if !has_model_flag(&wrapper_args) {
                if let Some(model) = &self.config.model {
                    cmd.arg("--model").arg(model);
                }
            }

            if self.config.verbose && !wrapper_args.iter().any(|arg| arg == "--print-logs") {
                cmd.arg("--print-logs");
            }

            cmd.args(&wrapper_args);
            cmd.arg(formatted_input.clone());
        } else if flavor == CliCommandFlavor::Codex {
            let (wrapper_args, has_prompt_source) =
                strip_codex_wrapper_args(&self.config.base.args);
            cmd.stdin(Stdio::piped());
            cmd.arg("exec");
            if let Some(session_id) = &self.config.base.resume_session_id {
                cmd.arg("resume").arg(session_id);
            }
            cmd.arg("--json");
            cmd.args(&wrapper_args);

            if !has_model_flag(&wrapper_args) {
                if let Some(model) = &self.config.model {
                    cmd.arg("--model").arg(model);
                }
            }

            if !has_prompt_source {
                cmd.arg("-");
            }
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

            let stdout_tracker = LiveJsonRuntimeTracker::new(
                flavor,
                self.config.base.name.clone(),
                worktree.clone(),
                &self.config.base.env,
            );

            let stdout_handle =
                std::thread::spawn(move || read_stream_to_string(stdout, stdout_tracker));
            let stderr_handle = std::thread::spawn(move || {
                let (out, _) = read_stream_to_string(stderr, None);
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
        let (mut stdout_content, live_tracker) = stdout_handle
            .join()
            .unwrap_or_else(|_| (String::new(), None));
        let mut stderr_content = stderr_handle.join().unwrap_or_else(|_| String::new());
        let mut structured_runtime_observations = Vec::new();

        if let Some(tracker) = live_tracker {
            let (mut observations, warnings) = tracker.finish();
            structured_runtime_observations.append(&mut observations);
            if !warnings.is_empty() {
                if !stderr_content.is_empty() && !stderr_content.ends_with('\n') {
                    stderr_content.push('\n');
                }
                stderr_content.push_str(&warnings.join("\n"));
                stderr_content.push('\n');
            }
        }

        match flavor {
            CliCommandFlavor::OpenCodeFamily => {
                let parsed = transform_json_output(
                    JsonAdapterKind::OpenCode,
                    &stdout_content,
                    &stderr_content,
                );
                stdout_content = parsed.stdout;
                stderr_content = parsed.stderr;
                structured_runtime_observations.extend(parsed.structured_runtime_observations);
            }
            CliCommandFlavor::Codex => {
                let parsed =
                    transform_json_output(JsonAdapterKind::Codex, &stdout_content, &stderr_content);
                stdout_content = parsed.stdout;
                stderr_content = parsed.stderr;
                structured_runtime_observations.extend(parsed.structured_runtime_observations);
            }
            CliCommandFlavor::Generic => {}
        }

        self.process = None;

        let exit_code = status.code().unwrap_or(-1);
        if exit_code == 0 {
            Ok(
                ExecutionReport::success(duration, stdout_content, stderr_content)
                    .with_structured_runtime_observations(structured_runtime_observations),
            )
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
            )
            .with_structured_runtime_observations(structured_runtime_observations))
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
    use std::path::Path;
    use tempfile::tempdir;

    fn init_git_repo(path: &Path) {
        std::fs::create_dir_all(path).expect("create repo dir");
        let run = |args: &[&str]| {
            let output = Command::new("git")
                .current_dir(path)
                .args(args)
                .output()
                .expect("run git command");
            assert!(output.status.success(), "git {args:?} failed");
        };

        run(&["init", "--initial-branch=main"]);
        run(&["config", "user.name", "Test User"]);
        run(&["config", "user.email", "test@example.com"]);
        std::fs::write(path.join("README.md"), "base\n").expect("write base file");
        run(&["add", "README.md"]);
        run(&["commit", "-m", "initial"]);
    }

    #[test]
    fn live_tracker_emits_session_and_turn_ref_for_opencode() {
        let tmp = tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        init_git_repo(&repo);
        std::fs::write(repo.join("turn.txt"), "turn one\n").expect("write turn file");

        let env = std::collections::HashMap::from([
            (
                "HIVEMIND_ATTEMPT_ID".to_string(),
                Uuid::new_v4().to_string(),
            ),
            ("HIVEMIND_TASK_ID".to_string(), Uuid::new_v4().to_string()),
        ]);
        let mut tracker = LiveJsonRuntimeTracker::new(
            CliCommandFlavor::OpenCodeFamily,
            "opencode".to_string(),
            repo.clone(),
            &env,
        )
        .expect("tracker");

        tracker.observe_stdout_line(r#"{"type":"message_part_updated","sessionID":"sess-123"}"#);
        tracker.observe_stdout_line(
            r#"{"type":"step_finish","sessionID":"sess-123","snapshot":"snap-1"}"#,
        );

        let (observations, warnings) = tracker.finish();
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert!(matches!(
            &observations[0],
            StructuredRuntimeObservation::SessionObserved { session_id, .. } if session_id == "sess-123"
        ));
        match &observations[1] {
            StructuredRuntimeObservation::TurnCompleted {
                ordinal,
                provider_session_id,
                provider_turn_id,
                git_ref,
                commit_sha,
                ..
            } => {
                assert_eq!(*ordinal, 1);
                assert_eq!(provider_session_id.as_deref(), Some("sess-123"));
                assert_eq!(provider_turn_id.as_deref(), Some("snap-1"));
                let git_ref = git_ref.as_ref().expect("git ref");
                let commit_sha = commit_sha.as_ref().expect("commit sha");
                let resolved = Command::new("git")
                    .current_dir(&repo)
                    .args(["rev-parse", git_ref])
                    .output()
                    .expect("resolve git ref");
                assert!(resolved.status.success(), "resolve ref failed");
                assert_eq!(String::from_utf8_lossy(&resolved.stdout).trim(), commit_sha);
            }
            other => panic!("unexpected observation: {other:?}"),
        }
    }
}
