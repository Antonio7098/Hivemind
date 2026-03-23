use super::*;

#[test]
fn exec_command_and_write_stdin_support_interactive_session() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: vec![],
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);

    let spawn = NativeToolAction {
        name: "exec_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "cmd":"sh",
            "args":["-c","IFS= read -r line; printf '%s\\n' \"$line\""]
        }),
    };
    let spawned = engine.execute(&spawn, &ctx).expect("spawn session");
    let spawned: ExecSessionOutput = serde_json::from_value(spawned).expect("decode spawn");
    assert!(spawned.session_id > 0);

    let write = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "session_id": spawned.session_id,
            "chars": "hello-session\n",
            "wait_ms": 120
        }),
    };
    let write_output = engine.execute(&write, &ctx).expect("write stdin");
    let write_output: ExecSessionOutput =
        serde_json::from_value(write_output).expect("decode write");
    assert!(write_output.stdout.contains("hello-session"));

    let close = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "session_id": spawned.session_id,
            "wait_ms": 120
        }),
    };
    let closed = engine.execute(&close, &ctx).expect("close stdin");
    let closed: ExecSessionOutput = serde_json::from_value(closed).expect("decode close");
    assert_eq!(closed.exit_code, Some(0));
    let _ = cleanup_exec_sessions();
}

#[test]
#[ignore = "timeout in CI due to 4.5 minute runner limit"]
fn write_stdin_reports_truncation_metadata() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: vec![],
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);

    let spawn = NativeToolAction {
        name: "exec_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "cmd":"sh",
            "args":["-c","IFS= read -r line; printf '%s\\n' \"$line\""]
        }),
    };
    let spawned = engine.execute(&spawn, &ctx).expect("spawn session");
    let spawned: ExecSessionOutput = serde_json::from_value(spawned).expect("decode spawn");

    let payload = "x".repeat(2048) + "\n";
    let write = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "session_id": spawned.session_id,
            "chars": payload,
            "wait_ms": 120,
            "max_bytes_per_stream": 64
        }),
    };
    let write_output = engine.execute(&write, &ctx).expect("write stdin");
    let write_output: ExecSessionOutput =
        serde_json::from_value(write_output).expect("decode write");
    assert!(write_output.stdout_truncated);
    assert!(write_output.stdout_truncated_bytes > 0);
    let _ = cleanup_exec_sessions();
}

#[test]
#[ignore = "timeout in CI due to 4.5 minute runner limit"]
fn exec_command_prunes_sessions_when_cap_exceeded() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sleep".to_string()],
        denylist: vec![],
        deny_by_default: true,
    };
    let mut env = HashMap::new();
    env.insert(EXEC_SESSION_CAP_ENV_KEY.to_string(), "2".to_string());
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);

    let spawn = |engine: &NativeToolEngine, ctx: &ToolExecutionContext<'_>| -> u64 {
        let action = NativeToolAction {
            name: "exec_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({"cmd":"sleep","args":["1"]}),
        };
        let out = engine.execute(&action, ctx).expect("spawn");
        let decoded: ExecSessionOutput = serde_json::from_value(out).expect("decode");
        decoded.session_id
    };
    let first = spawn(&engine, &ctx);
    let second = spawn(&engine, &ctx);
    let third = spawn(&engine, &ctx);
    assert!(third > second);

    let write_first = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({"session_id": first, "chars":"x"}),
    };
    let err = engine
        .execute(&write_first, &ctx)
        .expect_err("first session should be pruned");
    assert_eq!(err.code, "native_tool_input_invalid");
    let _ = cleanup_exec_sessions();
}

#[test]
fn write_stdin_rejects_cross_worktree_session_access() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let engine = NativeToolEngine::default();
    let tmp_a = tempfile::tempdir().expect("tempdir");
    let tmp_b = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: vec![],
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx_a = test_tool_context(tmp_a.path(), Some(&scope), &policy, &env);
    let ctx_b = test_tool_context(tmp_b.path(), Some(&scope), &policy, &env);

    let spawn = NativeToolAction {
        name: "exec_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({"cmd":"sh","args":["-c","sleep 1"]}),
    };
    let spawned = engine.execute(&spawn, &ctx_a).expect("spawn session");
    let spawned: ExecSessionOutput = serde_json::from_value(spawned).expect("decode spawn");

    let write = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({"session_id": spawned.session_id, "chars": "x"}),
    };
    let error = engine
        .execute(&write, &ctx_b)
        .expect_err("cross-worktree write_stdin should fail");
    assert_eq!(error.code, "native_scope_violation");
    let _ = cleanup_exec_sessions();
}

#[test]
fn exec_command_clamps_capture_wait_to_timeout_envelope() {
    let started = Instant::now();
    let wait_ms = clamp_exec_wait_ms(Some(5_200), DEFAULT_EXEC_SESSION_CAPTURE_MS, 5_000, started);
    assert!(
        (1..=4_950).contains(&wait_ms),
        "unexpected clamp value: {wait_ms}"
    );

    let exhausted = Instant::now()
        .checked_sub(Duration::from_millis(6_000))
        .expect("duration subtraction should succeed");
    let wait_ms = clamp_exec_wait_ms(
        Some(5_200),
        DEFAULT_EXEC_SESSION_CAPTURE_MS,
        5_000,
        exhausted,
    );
    assert_eq!(
        wait_ms, 1,
        "wait should clamp to floor when timeout is exhausted"
    );
}

#[test]
fn write_stdin_clamps_wait_to_timeout_envelope() {
    let started = Instant::now();
    let wait_ms = clamp_exec_wait_ms(
        Some(5_200),
        DEFAULT_EXEC_SESSION_WRITE_WAIT_MS,
        5_000,
        started,
    );
    assert!(
        (1..=4_950).contains(&wait_ms),
        "unexpected clamp value: {wait_ms}"
    );

    let default_wait = clamp_exec_wait_ms(
        None,
        DEFAULT_EXEC_SESSION_WRITE_WAIT_MS,
        5_000,
        Instant::now(),
    );
    assert_eq!(
        default_wait, DEFAULT_EXEC_SESSION_WRITE_WAIT_MS,
        "default write wait should be used when request is omitted"
    );
}

#[test]
fn validation_latency_baseline_is_bounded() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "read_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "oops": "invalid" }),
    };

    let samples = 500_u32;
    let started = Instant::now();
    for _ in 0..samples {
        let error = engine
            .execute(&action, &ctx)
            .expect_err("invalid payload should fail");
        assert_eq!(error.code, "native_tool_input_invalid");
    }
    let avg_us = started.elapsed().as_micros() / u128::from(samples);
    assert!(
        avg_us < 20_000,
        "validation baseline too slow: average {avg_us}us"
    );
}
