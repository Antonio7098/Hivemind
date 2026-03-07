use super::*;

#[derive(Debug)]
pub(super) struct StreamDelta {
    pub(super) content: String,
    pub(super) truncated: bool,
    pub(super) truncated_bytes: usize,
}

#[derive(Debug)]
pub(super) struct ExecSession {
    pub(super) session_id: u64,
    pub(super) owner_worktree: PathBuf,
    pub(super) command_line: String,
    pub(super) child: Child,
    pub(super) stdin: Option<ChildStdin>,
    pub(super) stdout_rx: Receiver<Vec<u8>>,
    pub(super) stderr_rx: Receiver<Vec<u8>>,
    pub(super) last_touched: Instant,
}

impl ExecSession {
    pub(super) fn touch(&mut self) {
        self.last_touched = Instant::now();
    }

    pub(super) fn exit_code(&mut self) -> Result<Option<i32>, NativeToolEngineError> {
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

    pub(super) fn terminate(&mut self) {
        terminate_child_process_group(&mut self.child);
    }
}

#[derive(Default)]
pub(super) struct ExecSessionManager {
    pub(super) sessions: BTreeMap<u64, ExecSession>,
    pub(super) order: VecDeque<u64>,
}

impl ExecSessionManager {
    pub(super) fn touch(&mut self, session_id: u64) {
        self.order.retain(|id| *id != session_id);
        self.order.push_back(session_id);
        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.touch();
        }
    }
}

static EXEC_SESSION_MANAGER: OnceLock<Mutex<ExecSessionManager>> = OnceLock::new();
pub(super) static EXEC_SESSION_NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn exec_session_manager() -> &'static Mutex<ExecSessionManager> {
    EXEC_SESSION_MANAGER.get_or_init(|| Mutex::new(ExecSessionManager::default()))
}

pub(super) fn spawn_pipe_reader(mut stream: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
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

pub(super) fn collect_stream_delta(
    rx: &Receiver<Vec<u8>>,
    wait_ms: u64,
    max_bytes: usize,
) -> StreamDelta {
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

pub(super) fn parse_session_cap(env: &HashMap<String, String>) -> usize {
    env.get(EXEC_SESSION_CAP_ENV_KEY)
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_EXEC_SESSION_CAP)
}

pub(super) fn prune_sessions_if_needed(manager: &mut ExecSessionManager, cap: usize) {
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

pub(super) fn resolve_session_cwd(
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

pub(super) fn session_owner_worktree(worktree: &Path) -> PathBuf {
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

pub(super) fn session_output(
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
