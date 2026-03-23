use super::*;

#[test]
fn network_policy_denylist_precedes_allowlist() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        allowlist: vec!["example.com".to_string()],
        denylist: vec!["example.com".to_string()],
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "echo",
            "args": ["https://example.com/resource"]
        }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("denylist must win over allowlist");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "network_decision:denied_denylist"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn network_policy_blocks_private_host_addresses() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        block_private_addresses: true,
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "echo",
            "args": ["http://127.0.0.1:8080/health"]
        }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("private host should be blocked");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "network_decision:denied_private_address"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn network_policy_limited_mode_restricts_methods() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        access_mode: NativeNetworkAccessMode::Limited,
        limited_methods: vec!["GET".to_string()],
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "echo",
            "args": ["-X", "POST", "https://example.com/resource"]
        }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("limited mode should deny unlisted methods");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "network_decision:denied_method_restricted"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn network_immediate_approval_is_cached_for_session() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        approval_mode: NativeNetworkApprovalMode::Immediate,
        approval_decision: NativeNetworkApprovalDecision::Approve,
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "echo",
            "args": ["https://example.com/resource"]
        }),
    };

    let first = engine.execute_action_trace("network-immediate-1".to_string(), &action, &ctx);
    assert!(first.failure.is_none(), "{first:?}");
    assert!(
        first
            .policy_tags
            .iter()
            .any(|tag| tag == "network_approval_outcome:approved_for_session"),
        "{:?}",
        first.policy_tags
    );

    let second = engine.execute_action_trace("network-immediate-2".to_string(), &action, &ctx);
    assert!(second.failure.is_none(), "{second:?}");
    assert!(
        second
            .policy_tags
            .iter()
            .any(|tag| tag == "network_approval_outcome:approved_cached"),
        "{:?}",
        second.policy_tags
    );
}

#[test]
fn deferred_network_denial_terminates_running_command() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let decisions_path = tmp.path().join("network-decisions.log");
    fs::write(&decisions_path, "").expect("seed decisions file");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        approval_mode: NativeNetworkApprovalMode::Deferred,
        deferred_decisions_file: Some(decisions_path.to_string_lossy().to_string()),
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let writer_path = decisions_path;
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        fs::write(writer_path, "https://example.com:443,deny\n").expect("write denial");
    });

    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "sh",
            "args": ["-c", "sleep 5", "https://example.com"],
            "timeout_ms": 5000
        }),
    };
    let error = engine
        .execute(&action, &ctx)
        .expect_err("deferred denial should terminate command");
    writer.join().expect("writer thread");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "network_approval_outcome:deferred_denied"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn managed_proxy_bind_is_clamped_without_dangerous_override() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let env = HashMap::new();
    let network_policy = NativeNetworkPolicy {
        proxy_mode: NativeNetworkProxyMode::Managed,
        proxy_http_bind: "0.0.0.0:0".to_string(),
        proxy_admin_bind: "0.0.0.0:0".to_string(),
        ..NativeNetworkPolicy::default()
    };
    let ctx = test_tool_context_with_network_policy(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        network_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "echo",
            "args": ["https://example.com/resource"]
        }),
    };

    let trace = engine.execute_action_trace("managed-proxy-check".to_string(), &action, &ctx);
    assert!(trace.failure.is_none(), "{trace:?}");
    assert!(
        trace
            .policy_tags
            .iter()
            .any(|tag| tag == "network_proxy_bind_clamped:true"),
        "{:?}",
        trace.policy_tags
    );
}
