use super::super::graph_query_tool::mark_runtime_graph_dirty;
use super::*;
use support::{
    clamp_exec_wait_ms, collect_stream_delta, exec_session_manager, parse_session_cap,
    prune_sessions_if_needed, resolve_session_cwd, session_output, session_owner_worktree,
    spawn_pipe_reader, EXEC_SESSION_NEXT_ID,
};

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
        support::ExecSession {
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

    mark_runtime_graph_dirty(ctx.env, &[]);
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

    if input.chars.is_some() {
        mark_runtime_graph_dirty(ctx.env, &[]);
    }
    session_output(input.session_id, exit_code, stdout, stderr, warnings)
}
