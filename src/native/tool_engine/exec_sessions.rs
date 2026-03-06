use super::*;

pub(super) const DEFAULT_EXEC_SESSION_CAPTURE_MS: u64 = 80;
pub(super) const DEFAULT_EXEC_SESSION_WRITE_WAIT_MS: u64 = 80;
const DEFAULT_EXEC_SESSION_STREAM_MAX_BYTES: usize = 16_384;
const DEFAULT_EXEC_SESSION_CAP: usize = 24;
const PROTECTED_RECENT_SESSION_COUNT: usize = 4;
pub(super) const EXEC_SESSION_CAP_ENV_KEY: &str = "HIVEMIND_NATIVE_EXEC_SESSION_CAP";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecCommandInput {
    pub(super) cmd: String,
    #[serde(default)]
    pub(super) args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    initial_input: Option<String>,
    #[serde(default)]
    capture_ms: Option<u64>,
    #[serde(default)]
    max_bytes_per_stream: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct WriteStdinInput {
    session_id: u64,
    #[serde(default)]
    chars: Option<String>,
    #[serde(default)]
    wait_ms: Option<u64>,
    #[serde(default)]
    max_bytes_per_stream: Option<usize>,
    #[serde(default)]
    close_stdin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ExecSessionOutput {
    pub(super) session_id: u64,
    #[serde(default)]
    pub(super) exit_code: Option<i32>,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) stdout_truncated: bool,
    pub(super) stderr_truncated: bool,
    pub(super) stdout_truncated_bytes: usize,
    pub(super) stderr_truncated_bytes: usize,
    #[serde(default)]
    pub(super) warnings: Vec<String>,
}

#[derive(Debug)]
struct StreamDelta {
    content: String,
    truncated: bool,
    truncated_bytes: usize,
}

#[derive(Debug)]
struct ExecSession {
    session_id: u64,
    owner_worktree: PathBuf,
    command_line: String,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout_rx: Receiver<Vec<u8>>,
    stderr_rx: Receiver<Vec<u8>>,
    last_touched: Instant,
}

impl ExecSession {
    fn touch(&mut self) {
        self.last_touched = Instant::now();
    }

    fn exit_code(&mut self) -> Result<Option<i32>, NativeToolEngineError> {
        self.child
            .try_wait()
            .map(|status| status.map(|s| s.code().unwrap_or(-1)))
            .map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed waiting on exec session {}: {error}",
                    self.session_id
                ))
            })
    }

    fn terminate(&mut self) {
        terminate_child_process_group(&mut self.child);
    }
}

#[derive(Default)]
struct ExecSessionManager {
    sessions: BTreeMap<u64, ExecSession>,
    order: VecDeque<u64>,
}

impl ExecSessionManager {
    fn touch(&mut self, session_id: u64) {
        self.order.retain(|id| *id != session_id);
        self.order.push_back(session_id);
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.touch();
        }
    }
}

static EXEC_SESSION_MANAGER: OnceLock<Mutex<ExecSessionManager>> = OnceLock::new();
static EXEC_SESSION_NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn exec_session_manager() -> &'static Mutex<ExecSessionManager> {
    EXEC_SESSION_MANAGER.get_or_init(|| Mutex::new(ExecSessionManager::default()))
}

fn spawn_pipe_reader(mut stream: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0_u8; 4096];
        loop {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

fn collect_stream_delta(rx: &Receiver<Vec<u8>>, wait_ms: u64, max_bytes: usize) -> StreamDelta {
    let deadline = Instant::now() + Duration::from_millis(wait_ms.max(1));
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut truncated_bytes = 0usize;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(10))) {
            Ok(chunk) => {
                if buf.len() < max_bytes {
                    let remaining_capacity = max_bytes - buf.len();
                    if chunk.len() <= remaining_capacity {
                        buf.extend_from_slice(&chunk);
                    } else {
                        buf.extend_from_slice(&chunk[..remaining_capacity]);
                        truncated = true;
                        truncated_bytes += chunk.len() - remaining_capacity;
                    }
                } else {
                    truncated = true;
                    truncated_bytes += chunk.len();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    StreamDelta {
        content: String::from_utf8_lossy(&buf).to_string(),
        truncated,
        truncated_bytes,
    }
}

#[must_use]
pub fn cleanup_exec_sessions() -> usize {
    let lock = exec_session_manager();
    let mut manager = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut cleaned = 0usize;
    for session in manager.sessions.values_mut() {
        session.terminate();
        cleaned += 1;
    }
    manager.sessions.clear();
    manager.order.clear();
    cleaned
}

fn parse_session_cap(env: &HashMap<String, String>) -> usize {
    env.get(EXEC_SESSION_CAP_ENV_KEY)
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_EXEC_SESSION_CAP)
}

fn prune_sessions_if_needed(manager: &mut ExecSessionManager, cap: usize) {
    while manager.sessions.len() > cap {
        let protected_count = PROTECTED_RECENT_SESSION_COUNT.min(cap.saturating_sub(1));
        let protected = manager
            .order
            .iter()
            .rev()
            .take(protected_count)
            .copied()
            .collect::<Vec<_>>();
        let mut candidate: Option<u64> = None;
        for id in manager.order.iter().copied() {
            if protected.contains(&id) {
                continue;
            }
            if let Some(session) = manager.sessions.get_mut(&id) {
                let exited = session.exit_code().ok().flatten().is_some();
                if exited {
                    candidate = Some(id);
                    break;
                }
                if candidate.is_none() {
                    candidate = Some(id);
                }
            }
        }
        let id = candidate.or_else(|| manager.order.front().copied());
        let Some(id) = id else { break };
        if let Some(mut removed) = manager.sessions.remove(&id) {
            removed.terminate();
        }
        manager.order.retain(|value| *value != id);
    }
}

fn resolve_session_cwd(
    worktree: &Path,
    cwd: Option<&str>,
) -> Result<PathBuf, NativeToolEngineError> {
    let Some(raw) = cwd else {
        return Ok(worktree.to_path_buf());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(worktree.to_path_buf());
    }
    let rel = normalize_relative_path(trimmed, true)?;
    Ok(worktree.join(rel))
}

fn session_owner_worktree(worktree: &Path) -> PathBuf {
    fs::canonicalize(worktree).unwrap_or_else(|_| worktree.to_path_buf())
}

pub(super) fn clamp_exec_wait_ms(
    requested: Option<u64>,
    default_wait_ms: u64,
    timeout_ms: u64,
    started: Instant,
) -> u64 {
    let requested = requested.unwrap_or(default_wait_ms);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let cap = timeout_ms
        .saturating_sub(elapsed_ms.saturating_add(50))
        .max(1);
    requested.min(cap)
}

fn session_output(
    session_id: u64,
    exit_code: Option<i32>,
    stdout: StreamDelta,
    stderr: StreamDelta,
    warnings: Vec<String>,
) -> Result<Value, NativeToolEngineError> {
    serde_json::to_value(ExecSessionOutput {
        session_id,
        exit_code,
        stdout: stdout.content,
        stderr: stderr.content,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
        stdout_truncated_bytes: stdout.truncated_bytes,
        stderr_truncated_bytes: stderr.truncated_bytes,
        warnings,
    })
    .map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode exec session output: {error}"))
    })
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_exec_command(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let started = Instant::now();
    let input = decode_input::<ExecCommandInput>(input)?;
    let raw_command = input.cmd.trim();
    if raw_command.is_empty() {
        return Err(NativeToolEngineError::validation(
            "exec_command requires a non-empty cmd",
        ));
    }
    let command = normalize_exec_command(raw_command, ctx.env);
    let command_line = if input.args.is_empty() {
        command.clone()
    } else {
        format!("{command} {}", input.args.join(" "))
    };
    let raw_command_line = if input.args.is_empty() {
        raw_command.to_string()
    } else {
        format!("{raw_command} {}", input.args.join(" "))
    };
    if let Some(scope) = ctx.scope {
        if !scope.execution.is_allowed(&command_line)
            && (raw_command_line == command_line || !scope.execution.is_allowed(&raw_command_line))
        {
            return Err(NativeToolEngineError::scope_violation(format!(
                "exec_command blocked by execution scope: {command_line}"
            )));
        }
    }

    let cwd = resolve_session_cwd(ctx.worktree, input.cwd.as_deref())?;
    let mut cmd = Command::new(&command);
    cmd.args(&input.args)
        .current_dir(cwd)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|error| {
        NativeToolEngineError::execution(format!("failed to execute '{command_line}': {error}"))
    })?;
    let stdout: ChildStdout = child.stdout.take().ok_or_else(|| {
        NativeToolEngineError::execution("exec_command failed to capture child stdout")
    })?;
    let stderr: ChildStderr = child.stderr.take().ok_or_else(|| {
        NativeToolEngineError::execution("exec_command failed to capture child stderr")
    })?;
    let mut stdin = child.stdin.take();
    if let Some(initial) = input.initial_input.as_ref() {
        if let Some(writer) = stdin.as_mut() {
            writer.write_all(initial.as_bytes()).map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to write initial_input for '{command_line}': {error}"
                ))
            })?;
            writer.flush().map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to flush initial_input for '{command_line}': {error}"
                ))
            })?;
        }
    }
    let stdout_rx = spawn_pipe_reader(stdout);
    let stderr_rx = spawn_pipe_reader(stderr);

    let session_id = EXEC_SESSION_NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let cap = parse_session_cap(ctx.env);
    let owner_worktree = session_owner_worktree(ctx.worktree);
    let mut warnings = Vec::new();

    let lock = exec_session_manager();
    let mut manager = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    manager.sessions.insert(
        session_id,
        ExecSession {
            session_id,
            owner_worktree,
            command_line,
            child,
            stdin,
            stdout_rx,
            stderr_rx,
            last_touched: Instant::now(),
        },
    );
    manager.touch(session_id);
    if manager.sessions.len() >= cap.saturating_mul(8) / 10 {
        warnings.push(format!(
            "exec session count {} is approaching cap {}",
            manager.sessions.len(),
            cap
        ));
    }
    prune_sessions_if_needed(&mut manager, cap);

    let max_bytes = input
        .max_bytes_per_stream
        .unwrap_or(DEFAULT_EXEC_SESSION_STREAM_MAX_BYTES);
    let session = manager
        .sessions
        .get_mut(&session_id)
        .ok_or_else(|| NativeToolEngineError::execution("session disappeared after spawn"))?;
    let stdout_wait_ms = clamp_exec_wait_ms(
        input.capture_ms,
        DEFAULT_EXEC_SESSION_CAPTURE_MS,
        default_timeout_ms,
        started,
    );
    let stdout = collect_stream_delta(&session.stdout_rx, stdout_wait_ms, max_bytes);
    let stderr_wait_ms = clamp_exec_wait_ms(
        input.capture_ms,
        DEFAULT_EXEC_SESSION_CAPTURE_MS,
        default_timeout_ms,
        started,
    );
    let stderr = collect_stream_delta(&session.stderr_rx, stderr_wait_ms, max_bytes);
    let exit_code = session.exit_code()?;
    drop(manager);

    session_output(session_id, exit_code, stdout, stderr, warnings)
}

pub(super) fn handle_write_stdin(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let started = Instant::now();
    let input = decode_input::<WriteStdinInput>(input)?;
    let owner_worktree = session_owner_worktree(ctx.worktree);
    let lock = exec_session_manager();
    let mut manager = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (stdout, stderr, exit_code) = {
        let session = manager.sessions.get_mut(&input.session_id).ok_or_else(|| {
            NativeToolEngineError::validation(format!(
                "unknown exec session id {}",
                input.session_id
            ))
        })?;
        if session.owner_worktree != owner_worktree {
            return Err(NativeToolEngineError::scope_violation(format!(
                "exec session {} belongs to a different worktree",
                input.session_id
            )));
        }
        if let Some(chars) = input.chars.as_ref() {
            if let Some(stdin) = session.stdin.as_mut() {
                stdin.write_all(chars.as_bytes()).map_err(|error| {
                    NativeToolEngineError::execution(format!(
                        "failed writing to exec session {} ({}): {error}",
                        session.session_id, session.command_line
                    ))
                })?;
                stdin.flush().map_err(|error| {
                    NativeToolEngineError::execution(format!(
                        "failed flushing exec session {}: {error}",
                        session.session_id
                    ))
                })?;
            } else {
                return Err(NativeToolEngineError::execution(format!(
                    "stdin is already closed for exec session {}",
                    session.session_id
                )));
            }
        }
        if input.close_stdin {
            session.stdin = None;
        }

        let max_bytes = input
            .max_bytes_per_stream
            .unwrap_or(DEFAULT_EXEC_SESSION_STREAM_MAX_BYTES);
        let stdout_wait_ms = clamp_exec_wait_ms(
            input.wait_ms,
            DEFAULT_EXEC_SESSION_WRITE_WAIT_MS,
            default_timeout_ms,
            started,
        );
        let stdout = collect_stream_delta(&session.stdout_rx, stdout_wait_ms, max_bytes);
        let stderr_wait_ms = clamp_exec_wait_ms(
            input.wait_ms,
            DEFAULT_EXEC_SESSION_WRITE_WAIT_MS,
            default_timeout_ms,
            started,
        );
        let stderr = collect_stream_delta(&session.stderr_rx, stderr_wait_ms, max_bytes);
        let exit_code = session.exit_code()?;
        if exit_code.is_some() {
            let _ = session.stdin.take();
        }
        (stdout, stderr, exit_code)
    };
    manager.touch(input.session_id);

    let cap = parse_session_cap(ctx.env);
    let mut warnings = Vec::new();
    if manager.sessions.len() >= cap.saturating_mul(8) / 10 {
        warnings.push(format!(
            "exec session count {} is approaching cap {}",
            manager.sessions.len(),
            cap
        ));
    }
    drop(manager);

    session_output(input.session_id, exit_code, stdout, stderr, warnings)
}
