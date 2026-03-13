use super::*;
use crate::core::scope::{
    ExecutionScope, FilePermission, FilesystemScope, PathRule, RepositoryScope, Scope,
};
use proptest::prelude::*;
use std::sync::{Mutex, MutexGuard, OnceLock};
use ucp_api::{
    build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig, CODEGRAPH_EXTRACTOR_VERSION,
};

static EXEC_SESSION_TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_exec_session_tests() -> MutexGuard<'static, ()> {
    EXEC_SESSION_TEST_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn init_git_repo(path: &Path) {
    fs::create_dir_all(path).expect("create repo dir");
    let output = Command::new("git")
        .args(["init"])
        .current_dir(path)
        .output()
        .expect("git init");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_commit_all(path: &Path, message: &str) {
    let add = Command::new("git")
        .args(["add", "."])
        .current_dir(path)
        .output()
        .expect("git add");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let commit = Command::new("git")
        .args([
            "-c",
            "user.name=Hivemind",
            "-c",
            "user.email=hivemind@example.com",
            "commit",
            "-m",
            message,
        ])
        .current_dir(path)
        .output()
        .expect("git commit");
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

fn git_head(path: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .expect("git rev-parse");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn sqlite_scalar_string(db_path: &Path, sql: &str) -> String {
    let output = Command::new("sqlite3")
        .arg("-noheader")
        .arg("-batch")
        .arg(db_path)
        .arg(sql)
        .output()
        .expect("sqlite3 query");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[cfg(unix)]
fn write_executable(path: &Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).expect("write executable file");
    let mut permissions = fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("set executable permissions");
}

fn write_snapshot_artifact(repo_path: &Path, snapshot_path: &Path) {
    let commit_hash = git_head(repo_path);
    let built = build_code_graph(&CodeGraphBuildInput {
        repository_path: repo_path.to_path_buf(),
        commit_hash: commit_hash.clone(),
        config: CodeGraphExtractorConfig::default(),
    })
    .expect("build code graph");
    let portable = PortableDocument::from_document(&built.document);
    let repositories = vec![RuntimeGraphSnapshotRepository {
        repo_name: "repo".to_string(),
        repo_path: repo_path.to_string_lossy().to_string(),
        commit_hash: commit_hash.clone(),
        canonical_fingerprint: built.canonical_fingerprint.clone(),
        document: portable,
    }];
    let artifact = RuntimeGraphSnapshotArtifact {
        profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
        canonical_fingerprint: aggregate_snapshot_fingerprint_registry_style(&repositories),
        provenance: RuntimeGraphSnapshotProvenance {
            head_commits: vec![RuntimeGraphSnapshotCommit {
                repo_name: "repo".to_string(),
                repo_path: repo_path.to_string_lossy().to_string(),
                commit_hash,
            }],
        },
        repositories,
    };
    let raw = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "graph_snapshot.v1",
        "snapshot_version": 1,
        "provenance": {
            "project_id": uuid::Uuid::new_v4(),
            "head_commits": artifact.provenance.head_commits,
            "generated_at": chrono::Utc::now(),
        },
        "ucp_engine_version": CODEGRAPH_EXTRACTOR_VERSION,
        "profile_version": artifact.profile_version,
        "canonical_fingerprint": artifact.canonical_fingerprint,
        "summary": {
            "total_nodes": built.stats.total_nodes,
            "repository_nodes": built.stats.repository_nodes,
            "directory_nodes": built.stats.directory_nodes,
            "file_nodes": built.stats.file_nodes,
            "symbol_nodes": built.stats.symbol_nodes,
            "total_edges": built.stats.total_edges,
            "reference_edges": built.stats.reference_edges,
            "export_edges": built.stats.export_edges,
            "languages": built.stats.languages,
        },
        "repositories": artifact.repositories,
        "static_projection": "",
    }))
    .expect("serialize snapshot");
    fs::write(snapshot_path, raw).expect("write snapshot");
}

fn allow_all_scope() -> Scope {
    Scope::new()
        .with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("*", FilePermission::Write)),
        )
        .with_execution(ExecutionScope::new().allow("*"))
}

fn test_tool_context<'a>(
    worktree: &'a Path,
    scope: Option<&'a Scope>,
    policy: &NativeCommandPolicy,
    env: &'a HashMap<String, String>,
) -> ToolExecutionContext<'a> {
    test_tool_context_with_policies(
        worktree,
        scope,
        policy,
        env,
        NativeSandboxPolicy::default(),
        NativeApprovalPolicy::default(),
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    )
}

fn test_tool_context_with_policies<'a>(
    worktree: &'a Path,
    scope: Option<&'a Scope>,
    policy: &NativeCommandPolicy,
    env: &'a HashMap<String, String>,
    sandbox_policy: NativeSandboxPolicy,
    approval_policy: NativeApprovalPolicy,
    exec_policy_manager: NativeExecPolicyManager,
) -> ToolExecutionContext<'a> {
    test_tool_context_with_network_policy(
        worktree,
        scope,
        policy,
        env,
        sandbox_policy,
        approval_policy,
        NativeNetworkPolicy::default(),
        exec_policy_manager,
    )
}

#[allow(clippy::too_many_arguments)]
fn test_tool_context_with_network_policy<'a>(
    worktree: &'a Path,
    scope: Option<&'a Scope>,
    policy: &NativeCommandPolicy,
    env: &'a HashMap<String, String>,
    sandbox_policy: NativeSandboxPolicy,
    approval_policy: NativeApprovalPolicy,
    network_policy: NativeNetworkPolicy,
    exec_policy_manager: NativeExecPolicyManager,
) -> ToolExecutionContext<'a> {
    ToolExecutionContext {
        worktree,
        scope,
        sandbox_policy,
        approval_policy,
        network_policy,
        command_policy: policy.clone(),
        exec_policy_manager,
        approval_cache: RefCell::new(NativeApprovalCache::default()),
        network_approval_cache: RefCell::new(NativeNetworkApprovalCache::default()),
        env,
    }
}

#[test]
fn rejects_unknown_tool_names() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "nope".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({}),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("unknown tool should fail");
    assert_eq!(error.code, "native_tool_unknown");
}

#[test]
fn rejects_invalid_input_schema() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "read_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "missing": "path" }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("invalid schema should fail");
    assert_eq!(error.code, "native_tool_input_invalid");
    assert!(error.recoverable);
}

#[test]
fn write_file_obeys_scope_gate() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = Scope::new().with_filesystem(
        FilesystemScope::new().with_rule(PathRule::new("src/", FilePermission::Read)),
    );
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "write_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "path": "src/main.rs", "content": "fn main() {}" }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("write should be blocked");
    assert_eq!(error.code, "native_scope_violation");
}

#[test]
fn git_status_reports_untracked_file() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::write(repo.join("untracked.txt"), "hello\n").expect("write file");

    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope().with_repository(RepositoryScope::read_only(
        repo.to_string_lossy().to_string(),
    ));
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "git_status".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({}),
    };

    let output = engine
        .execute(&action, &ctx)
        .expect("git status should succeed");
    let rendered = output
        .get("output")
        .and_then(Value::as_str)
        .expect("git status output string");
    assert!(rendered.contains("?? untracked.txt"), "{rendered}");
}

#[test]
fn git_diff_reports_staged_changes() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::write(repo.join("tracked.txt"), "hello\n").expect("seed file");
    git_commit_all(&repo, "seed");
    fs::write(repo.join("tracked.txt"), "hello\nworld\n").expect("mutate file");

    let add = Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&repo)
        .output()
        .expect("git add tracked.txt");
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope().with_repository(RepositoryScope::read_only(
        repo.to_string_lossy().to_string(),
    ));
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "git_diff".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "staged": true }),
    };

    let output = engine
        .execute(&action, &ctx)
        .expect("git diff should succeed");
    let rendered = output
        .get("output")
        .and_then(Value::as_str)
        .expect("git diff output string");
    assert!(rendered.contains("tracked.txt"), "{rendered}");
    assert!(rendered.contains("+world"), "{rendered}");
}

#[test]
fn run_command_is_deny_by_default() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "echo", "args": ["hello"] }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("policy should deny command");
    assert_eq!(error.code, "native_policy_violation");
    assert!(error.recoverable);
}

#[test]
fn run_command_respects_allowlist_policy() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "echo", "args": ["hello"] }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("allowlisted command should run");
    let output: RunCommandOutput =
        serde_json::from_value(value).expect("run_command output should decode");
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("hello"));
}

#[test]
fn run_command_normalizes_cmd_alias_inline_args_and_wrapped_input() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "action": "run_command",
            "arguments": {
                "cmd": "echo normalized-command",
                "cwd": tmp.path().to_string_lossy().to_string()
            }
        }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("normalized run_command should run");
    let output: RunCommandOutput =
        serde_json::from_value(value).expect("run_command output should decode");
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "normalized-command");
}

#[test]
fn run_command_handler_splits_inline_command_strings_defensively() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);

    let value = super::run_command_tool::handle_run_command(
        &ctx,
        &json!({ "command": "echo handler-split" }),
        2_000,
    )
    .expect("handler should split inline command string");
    let output: RunCommandOutput = serde_json::from_value(value).expect("decode output");
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "handler-split");
}

#[test]
#[cfg(unix)]
fn exec_command_accepts_command_alias_and_absolute_worktree_cwd() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["pwd".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "exec_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "pwd",
            "cwd": tmp.path().to_string_lossy().to_string(),
            "capture_ms": 40
        }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("normalized exec_command should run");
    let output: ExecSessionOutput = serde_json::from_value(value).expect("decode exec output");
    assert!(output.session_id > 0);
    assert_eq!(output.stdout.trim(), tmp.path().to_string_lossy());
}

#[test]
#[cfg(unix)]
fn exec_command_handler_splits_inline_command_strings_defensively() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["pwd".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);

    let value = super::exec_sessions::handle_exec_command(
        &ctx,
        &json!({
            "cmd": format!("pwd --logical"),
            "capture_ms": 40
        }),
        2_000,
    )
    .expect("handler should split inline exec command string");
    let output: ExecSessionOutput = serde_json::from_value(value).expect("decode exec output");
    assert!(output.session_id > 0);
    assert_eq!(output.stdout.trim(), tmp.path().to_string_lossy());
}

#[test]
fn write_file_accepts_wrapped_arguments_and_absolute_worktree_paths() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let absolute = tmp.path().join("src/normalized.txt");
    let action = NativeToolAction {
        name: "write_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "action": "write_file",
            "arguments": {
                "path": absolute.to_string_lossy().to_string(),
                "content": "normalized-write"
            }
        }),
    };

    engine
        .execute(&action, &ctx)
        .expect("normalized write_file should succeed");
    assert_eq!(
        fs::read_to_string(absolute).expect("read normalized write"),
        "normalized-write"
    );
}

#[test]
#[cfg(unix)]
fn checkpoint_complete_uses_hivemind_bin_from_runtime_env() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin directory");
    let log_path = tmp.path().join("checkpoint.log");
    let script = bin.join("fake-hivemind");
    write_executable(
        &script,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            log_path.display()
        ),
    );

    let policy = NativeCommandPolicy::default();
    let mut env = HashMap::new();
    env.insert(
        "HIVEMIND_BIN".to_string(),
        script.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_DATA_DIR".to_string(),
        tmp.path().join("data").to_string_lossy().to_string(),
    );
    env.insert("HIVEMIND_ATTEMPT_ID".to_string(), "attempt-123".to_string());
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "checkpoint_complete".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "id": "checkpoint-1", "summary": "done" }),
    };

    let output = engine
        .execute(&action, &ctx)
        .expect("checkpoint_complete should succeed");
    let output: CheckpointCompleteOutput =
        serde_json::from_value(output).expect("checkpoint_complete output should decode");

    assert_eq!(output.checkpoint_id, "checkpoint-1");
    let invoked = fs::read_to_string(log_path).expect("checkpoint invocation log");
    assert!(invoked.contains("checkpoint"));
    assert!(invoked.contains("complete"));
    assert!(invoked.contains("--attempt-id"));
    assert!(invoked.contains("attempt-123"));
    assert!(invoked.contains("checkpoint-1"));
    assert!(invoked.contains("done"));
}

#[test]
#[cfg(unix)]
fn normalize_exec_command_uses_python3_when_python_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin directory");
    write_executable(&bin.join("python3"), "#!/bin/sh\nexit 0\n");

    let mut env = HashMap::new();
    env.insert("PATH".to_string(), bin.to_string_lossy().to_string());

    assert_eq!(normalize_exec_command("python", &env), "python3");
}

#[test]
#[cfg(unix)]
fn run_command_python_alias_falls_back_to_python3() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).expect("create bin directory");
    write_executable(
        &bin.join("python3"),
        "#!/bin/sh\nprintf 'python-alias-ok'\n",
    );

    let policy = NativeCommandPolicy {
        allowlist: vec!["python".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let mut env = HashMap::new();
    env.insert("PATH".to_string(), bin.to_string_lossy().to_string());
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "python", "args": [] }),
    };

    let output = engine
        .execute(&action, &ctx)
        .expect("python alias should resolve to python3");
    let output: RunCommandOutput =
        serde_json::from_value(output).expect("run_command output should decode");
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "python-alias-ok");
}

#[test]
fn run_command_uses_hardened_runtime_env_only() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let old = std::env::var("PARENT_SECRET").ok();
    std::env::set_var("PARENT_SECRET", "leak-me");

    let mut env = HashMap::new();
    env.insert("ONLY_THIS".to_string(), "visible".to_string());
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "sh",
            "args": ["-c", "printf '%s|%s' \"$ONLY_THIS\" \"$PARENT_SECRET\""]
        }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("allowlisted command should run");
    let output: RunCommandOutput =
        serde_json::from_value(value).expect("run_command output should decode");

    match old {
        Some(value) => std::env::set_var("PARENT_SECRET", value),
        None => std::env::remove_var("PARENT_SECRET"),
    }

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout, "visible|");
}

#[test]
fn sandbox_read_only_denies_write_tool() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let sandbox_policy = NativeSandboxPolicy {
        mode: NativeSandboxMode::ReadOnly,
        ..NativeSandboxPolicy::default()
    };
    let ctx = test_tool_context_with_policies(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        sandbox_policy,
        NativeApprovalPolicy::default(),
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "write_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "path": "src/blocked.txt", "content": "nope" }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("read-only sandbox should deny writes");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "sandbox_mode:read-only"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn approval_on_request_denies_when_review_is_deny() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let approval_policy = NativeApprovalPolicy {
        mode: NativeApprovalMode::OnRequest,
        review_decision: NativeApprovalReviewDecision::Deny,
        trusted_prefixes: Vec::new(),
        cache_max_entries: 8,
    };
    let ctx = test_tool_context_with_policies(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        approval_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "echo", "args": ["hello"] }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("on-request with deny decision should block");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error
            .policy_tags
            .iter()
            .any(|tag| tag == "approval_review_decision:deny"),
        "{:?}",
        error.policy_tags
    );
}

#[test]
fn approval_cache_marks_second_run_as_cached() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["echo".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let approval_policy = NativeApprovalPolicy {
        mode: NativeApprovalMode::OnRequest,
        review_decision: NativeApprovalReviewDecision::Approve,
        trusted_prefixes: Vec::new(),
        cache_max_entries: 8,
    };
    let ctx = test_tool_context_with_policies(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        approval_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "echo", "args": ["hello"] }),
    };

    let first = engine.execute_action_trace("call-1".to_string(), &action, &ctx);
    assert!(first.failure.is_none(), "{first:?}");
    assert!(
        first
            .policy_tags
            .iter()
            .any(|tag| tag == "approval_review_decision:approve"),
        "{:?}",
        first.policy_tags
    );

    let second = engine.execute_action_trace("call-2".to_string(), &action, &ctx);
    assert!(second.failure.is_none(), "{second:?}");
    assert!(
        second
            .policy_tags
            .iter()
            .any(|tag| tag == "approval_review_decision:cached"),
        "{:?}",
        second.policy_tags
    );
}

#[test]
fn dangerous_command_requires_danger_full_access_sandbox() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["rm".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let approval_policy = NativeApprovalPolicy {
        mode: NativeApprovalMode::OnRequest,
        review_decision: NativeApprovalReviewDecision::Approve,
        trusted_prefixes: Vec::new(),
        cache_max_entries: 8,
    };
    let sandbox_policy = NativeSandboxPolicy {
        mode: NativeSandboxMode::WorkspaceWrite,
        ..NativeSandboxPolicy::default()
    };
    let ctx = test_tool_context_with_policies(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        sandbox_policy,
        approval_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "rm", "args": ["-rf", "/"] }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("dangerous command must require elevated sandbox");
    assert_eq!(error.code, "native_policy_violation");
    assert!(
        error.message.contains("dangerous command denied"),
        "{}",
        error.message
    );
}

#[test]
fn approval_denies_broad_command_prefix_rules() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let approval_policy = NativeApprovalPolicy {
        mode: NativeApprovalMode::OnRequest,
        review_decision: NativeApprovalReviewDecision::Approve,
        trusted_prefixes: Vec::new(),
        cache_max_entries: 8,
    };
    let ctx = test_tool_context_with_policies(
        tmp.path(),
        Some(&scope),
        &policy,
        &env,
        NativeSandboxPolicy::default(),
        approval_policy,
        NativeExecPolicyManager {
            base: policy.clone(),
            ..NativeExecPolicyManager::default()
        },
    );
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "sh",
            "args": ["-c", "echo hello"]
        }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("broad shell prefix should be rejected");
    assert_eq!(error.code, "native_policy_violation");
    assert!(error.message.contains("broad prefix"), "{}", error.message);
}

#[test]
fn exec_prefix_amendments_are_bounded_and_filter_broad_prefixes() {
    let mut env = HashMap::new();
    env.insert(EXEC_PREFIX_RULE_MAX_ENV_KEY.to_string(), "2".to_string());
    env.insert(
        EXEC_PREFIX_AMENDMENTS_ENV_KEY.to_string(),
        "echo,*,sh,git status".to_string(),
    );
    let manager = NativeExecPolicyManager::from_env(&env);
    assert_eq!(manager.prefix_rule_max, 2);
    assert_eq!(
        manager.prefix_amendments,
        vec!["echo".to_string(), "git status".to_string()]
    );
}

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

#[test]
fn graph_query_tool_reads_snapshot_with_bounds() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    fs::write(repo.join("src/main.rs"), "fn main() { helper(); }\n").expect("write main");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let constitution_path = tmp.path().join("constitution.yaml");
    let state_db_path = tmp.path().join("native-state.sqlite");
    fs::write(
        &constitution_path,
        "partitions:\n  - id: core\n    path: src\n",
    )
    .expect("write constitution");

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        GRAPH_QUERY_ENV_CONSTITUTION_PATH.to_string(),
        constitution_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "node_type": "file",
            "path_prefix": "src",
            "max_results": 50
        }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("graph query should run");
    let result: GraphQueryResult = serde_json::from_value(value).expect("decode result");
    assert_eq!(result.query_kind, "filter");
    assert!(
        !result.nodes.is_empty(),
        "{}",
        serde_json::to_string(&result).unwrap_or_default()
    );
    assert!(result.duration_ms <= 5_000);
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT COUNT(1) FROM graphcode_artifacts WHERE project_id = 'project-graph-query';",
        ),
        "1"
    );
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT COUNT(1) FROM graphcode_sessions WHERE registry_key = 'project-graph-query:codegraph';",
        ),
        "1"
    );
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT storage_backend FROM graphcode_artifacts WHERE project_id = 'project-graph-query' LIMIT 1;",
        ),
        "ucp_graph_sqlite"
    );
    let storage_reference = sqlite_scalar_string(
        &state_db_path,
        "SELECT storage_reference FROM graphcode_artifacts WHERE project_id = 'project-graph-query' LIMIT 1;",
    );
    assert!(
        Path::new(&storage_reference).is_file(),
        "{storage_reference}"
    );
    let snapshot_json = sqlite_scalar_string(
        &state_db_path,
        "SELECT snapshot_json FROM graphcode_artifacts WHERE project_id = 'project-graph-query' LIMIT 1;",
    );
    assert!(
        !snapshot_json.contains("\"repositories\""),
        "{snapshot_json}"
    );
    let repo_manifest_json = sqlite_scalar_string(
        &state_db_path,
        "SELECT repo_manifest_json FROM graphcode_artifacts WHERE project_id = 'project-graph-query' LIMIT 1;",
    );
    assert!(
        repo_manifest_json.contains("worktree_path"),
        "{repo_manifest_json}"
    );
}

#[test]
fn graph_query_tool_fails_when_snapshot_is_stale() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    fs::write(
        repo.join("src/lib.rs"),
        "pub fn helper() { println!(\"x\"); }\n",
    )
    .expect("mutate repo");
    git_commit_all(&repo, "stale");

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-stale".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "max_results": 10
        }),
    };

    let error = engine
        .execute(&action, &ctx)
        .expect_err("stale snapshot should fail");
    assert_eq!(error.code, "graph_snapshot_stale");
    assert!(error.message.contains("hivemind graph snapshot refresh"));
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT freshness_state FROM graphcode_artifacts WHERE project_id = 'project-graph-query-stale' LIMIT 1;",
        ),
        "stale"
    );
}

#[test]
fn graph_query_tool_refreshes_attempt_scoped_execution_graph_incrementally() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-attempt".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert("HIVEMIND_ATTEMPT_ID".to_string(), "attempt-1".to_string());
    env.insert(
        "HIVEMIND_PRIMARY_WORKTREE".to_string(),
        repo.to_string_lossy().to_string(),
    );

    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let query_action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "path_prefix": "src",
            "max_results": 50
        }),
    };

    let initial: GraphQueryResult = serde_json::from_value(
        engine
            .execute(&query_action, &ctx)
            .expect("initial graph query should run"),
    )
    .expect("decode initial result");
    assert!(
        initial
            .nodes
            .iter()
            .any(|node| node.logical_key.contains("helper")),
        "{initial:?}"
    );
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT COUNT(1) FROM graphcode_artifacts WHERE project_id = 'project-graph-query-attempt';",
        ),
        "2"
    );

    let write_action = NativeToolAction {
        name: "write_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "path": "src/lib.rs",
            "content": "pub fn helper() {}\npub fn added() { helper(); }\n",
            "append": false
        }),
    };
    engine
        .execute(&write_action, &ctx)
        .expect("write_file should succeed");
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT freshness_state FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-attempt:codegraph:attempt:attempt-1';",
        ),
        "stale"
    );

    let refreshed: GraphQueryResult = serde_json::from_value(
        engine
            .execute(&query_action, &ctx)
            .expect("refreshed graph query should run"),
    )
    .expect("decode refreshed result");
    assert!(
        refreshed
            .nodes
            .iter()
            .any(|node| node.logical_key.contains("added")),
        "{refreshed:?}"
    );
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT freshness_state FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-attempt:codegraph:attempt:attempt-1';",
        ),
        "current"
    );
    let metadata_raw = sqlite_scalar_string(
        &state_db_path,
        "SELECT snapshot_json FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-attempt:codegraph:attempt:attempt-1';",
    );
    let metadata: Value = serde_json::from_str(&metadata_raw).expect("decode metadata");
    assert_eq!(
        metadata.get("graph_scope").and_then(Value::as_str),
        Some("execution_worktree")
    );
    assert_eq!(
        metadata.get("refresh_reason").and_then(Value::as_str),
        Some("incremental_refresh")
    );
    assert_eq!(
        metadata["incremental_repos"][0]["stats"]["requested"].as_bool(),
        Some(true)
    );
}

#[test]
fn graph_query_attempt_registry_dirty_helper_records_runtime_observed_paths() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-runtime-dirty".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert("HIVEMIND_ATTEMPT_ID".to_string(), "attempt-9".to_string());
    env.insert(
        "HIVEMIND_PRIMARY_WORKTREE".to_string(),
        repo.to_string_lossy().to_string(),
    );

    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let query_action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "path_prefix": "src",
            "max_results": 50
        }),
    };
    engine
        .execute(&query_action, &ctx)
        .expect("initial graph query should run");

    let store = crate::native::runtime_hardening::NativeRuntimeStateStore::open(
        &crate::native::runtime_hardening::RuntimeHardeningConfig::from_env(&env),
    )
    .expect("open runtime state store");
    mark_runtime_graph_registry_dirty(
        &store,
        "project-graph-query-runtime-dirty:codegraph:attempt:attempt-9",
        &[
            PathBuf::from("src/generated.rs"),
            PathBuf::from("src/lib.rs"),
        ],
        Some("runtime_filesystem_observed"),
    )
    .expect("mark runtime graph dirty by registry key");

    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT freshness_state FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-runtime-dirty:codegraph:attempt:attempt-9';",
        ),
        "stale"
    );
    let metadata_raw = sqlite_scalar_string(
        &state_db_path,
        "SELECT snapshot_json FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-runtime-dirty:codegraph:attempt:attempt-9';",
    );
    let metadata: Value = serde_json::from_str(&metadata_raw).expect("decode metadata");
    assert_eq!(
        metadata.get("refresh_reason").and_then(Value::as_str),
        Some("runtime_filesystem_observed")
    );
    let dirty_paths = metadata["dirty_paths"]
        .as_array()
        .expect("dirty_paths array");
    assert!(dirty_paths.iter().any(|value| value == "src/generated.rs"));
    assert!(dirty_paths.iter().any(|value| value == "src/lib.rs"));
}

#[test]
fn graph_query_refresh_selectively_invalidates_impacted_session_state() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/a.rs"), "pub fn alpha() {}\n").expect("write a");
    fs::write(repo.join("src/b.rs"), "pub fn beta() {}\n").expect("write b");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-session".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert("HIVEMIND_ATTEMPT_ID".to_string(), "attempt-2".to_string());
    env.insert(
        "HIVEMIND_PRIMARY_WORKTREE".to_string(),
        repo.to_string_lossy().to_string(),
    );

    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let query_action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "path_prefix": "src",
            "max_results": 50
        }),
    };
    engine
        .execute(&query_action, &ctx)
        .expect("initial graph query should run");

    let session_ref = "project-graph-query-session:codegraph:attempt:attempt-2:session:default";
    let before_raw = sqlite_scalar_string(
        &state_db_path,
        &format!(
            "SELECT working_set_refs_json FROM graphcode_sessions WHERE session_ref = '{session_ref}';"
        ),
    );
    let before: Vec<Value> = serde_json::from_str(&before_raw).expect("decode working set before");
    assert!(
        before.iter().any(|value| value["path"] == "src/a.rs"),
        "{before:?}"
    );
    assert!(
        before.iter().any(|value| value["path"] == "src/b.rs"),
        "{before:?}"
    );

    let write_action = NativeToolAction {
        name: "write_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "path": "src/a.rs",
            "content": "pub fn alpha() {}\npub fn alpha_two() {}\n",
            "append": false
        }),
    };
    engine
        .execute(&write_action, &ctx)
        .expect("write_file should succeed");

    crate::native::tool_engine::graph_query_tool::load_runtime_graph_snapshot(&env)
        .expect("execution graph refresh should succeed");

    let after_raw = sqlite_scalar_string(
        &state_db_path,
        &format!(
            "SELECT working_set_refs_json FROM graphcode_sessions WHERE session_ref = '{session_ref}';"
        ),
    );
    let after: Vec<Value> = serde_json::from_str(&after_raw).expect("decode working set after");
    assert!(
        after.iter().any(|value| value["path"] == "src/b.rs"),
        "{after:?}"
    );
    assert!(
        after.iter().all(|value| value["path"] != "src/a.rs"),
        "{after:?}"
    );
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            &format!(
                "SELECT freshness_state FROM graphcode_sessions WHERE session_ref = '{session_ref}';"
            ),
        ),
        "current"
    );
}

#[test]
fn run_command_records_dirty_paths_for_selective_graph_refresh() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/a.rs"), "pub fn alpha() {}\n").expect("write a");
    fs::write(repo.join("src/b.rs"), "pub fn beta() {}\n").expect("write b");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-run-command".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_ATTEMPT_ID".to_string(),
        "attempt-run-command".to_string(),
    );
    env.insert(
        "HIVEMIND_PRIMARY_WORKTREE".to_string(),
        repo.to_string_lossy().to_string(),
    );

    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let query_action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "path_prefix": "src",
            "max_results": 50
        }),
    };
    engine
        .execute(&query_action, &ctx)
        .expect("initial graph query should run");

    let run_action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "command": "sh",
            "args": [
                "-c",
                "printf 'pub fn alpha() {}\\npub fn alpha_two() {}\\n' > src/a.rs"
            ]
        }),
    };
    engine
        .execute(&run_action, &ctx)
        .expect("run_command should succeed");

    let metadata_raw = sqlite_scalar_string(
        &state_db_path,
        "SELECT snapshot_json FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-run-command:codegraph:attempt:attempt-run-command';",
    );
    let metadata: Value = serde_json::from_str(&metadata_raw).expect("decode metadata");
    let dirty_paths = metadata["dirty_paths"]
        .as_array()
        .expect("dirty_paths array");
    assert!(dirty_paths.iter().any(|value| value == "src/a.rs"));

    crate::native::tool_engine::graph_query_tool::load_runtime_graph_snapshot(&env)
        .expect("execution graph refresh should succeed");

    let session_ref =
        "project-graph-query-run-command:codegraph:attempt:attempt-run-command:session:default";
    let after_raw = sqlite_scalar_string(
        &state_db_path,
        &format!(
            "SELECT working_set_refs_json FROM graphcode_sessions WHERE session_ref = '{session_ref}';"
        ),
    );
    let after: Vec<Value> = serde_json::from_str(&after_raw).expect("decode working set after");
    assert!(
        after.iter().any(|value| value["path"] == "src/b.rs"),
        "{after:?}"
    );
    assert!(
        after.iter().all(|value| value["path"] != "src/a.rs"),
        "{after:?}"
    );
}

#[test]
fn write_stdin_records_dirty_paths_for_selective_graph_refresh() {
    let _guard = lock_exec_session_tests();
    let _ = cleanup_exec_sessions();

    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/a.rs"), "pub fn alpha() {}\n").expect("write a");
    fs::write(repo.join("src/b.rs"), "pub fn beta() {}\n").expect("write b");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy {
        allowlist: vec!["sh".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-write-stdin".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert(
        "HIVEMIND_ATTEMPT_ID".to_string(),
        "attempt-write-stdin".to_string(),
    );
    env.insert(
        "HIVEMIND_PRIMARY_WORKTREE".to_string(),
        repo.to_string_lossy().to_string(),
    );

    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let query_action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "filter",
            "path_prefix": "src",
            "max_results": 50
        }),
    };
    engine
        .execute(&query_action, &ctx)
        .expect("initial graph query should run");

    let spawn_action = NativeToolAction {
        name: "exec_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "cmd": "sh",
            "args": ["-c", "cat > src/a.rs"],
            "capture_ms": 40
        }),
    };
    let spawned = engine
        .execute(&spawn_action, &ctx)
        .expect("spawn exec session");
    let spawned: ExecSessionOutput = serde_json::from_value(spawned).expect("decode spawn");

    let write_action = NativeToolAction {
        name: "write_stdin".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "session_id": spawned.session_id,
            "chars": "pub fn alpha() {}\npub fn alpha_three() {}\n",
            "close_stdin": true,
            "wait_ms": 120
        }),
    };
    engine
        .execute(&write_action, &ctx)
        .expect("write_stdin should succeed");

    let metadata_raw = sqlite_scalar_string(
        &state_db_path,
        "SELECT snapshot_json FROM graphcode_artifacts WHERE registry_key = 'project-graph-query-write-stdin:codegraph:attempt:attempt-write-stdin';",
    );
    let metadata: Value = serde_json::from_str(&metadata_raw).expect("decode metadata");
    let dirty_paths = metadata["dirty_paths"]
        .as_array()
        .expect("dirty_paths array");
    assert!(dirty_paths.iter().any(|value| value == "src/a.rs"));

    crate::native::tool_engine::graph_query_tool::load_runtime_graph_snapshot(&env)
        .expect("execution graph refresh should succeed");

    let session_ref =
        "project-graph-query-write-stdin:codegraph:attempt:attempt-write-stdin:session:default";
    let after_raw = sqlite_scalar_string(
        &state_db_path,
        &format!(
            "SELECT working_set_refs_json FROM graphcode_sessions WHERE session_ref = '{session_ref}';"
        ),
    );
    let after: Vec<Value> = serde_json::from_str(&after_raw).expect("decode working set after");
    assert!(
        after.iter().any(|value| value["path"] == "src/b.rs"),
        "{after:?}"
    );
    assert!(
        after.iter().all(|value| value["path"] != "src/a.rs"),
        "{after:?}"
    );

    let _ = cleanup_exec_sessions();
}

#[test]
fn graph_query_tool_runs_python_query_when_ucp_python_is_available() {
    let python_bin = std::env::var("HIVEMIND_GRAPH_QUERY_TEST_PYTHON")
        .ok()
        .or_else(|| {
            let candidate = Path::new(env!("CARGO_MANIFEST_DIR")).join(".venv-ucp/bin/python");
            candidate
                .is_file()
                .then(|| candidate.to_string_lossy().to_string())
        });
    let Some(python_bin) = python_bin else {
        eprintln!("skipping python graph_query test: no UCP Python interpreter configured");
        return;
    };

    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    init_git_repo(&repo);
    fs::create_dir_all(repo.join("src")).expect("mkdir src");
    fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    fs::write(repo.join("src/main.rs"), "fn main() { helper(); }\n").expect("write main");
    git_commit_all(&repo, "seed");

    let snapshot_path = tmp.path().join("graph_snapshot.json");
    write_snapshot_artifact(&repo, &snapshot_path);

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let state_db_path = tmp.path().join("native-state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
    );
    env.insert(
        crate::core::graph_query::GRAPH_QUERY_ENV_PROJECT_ID.to_string(),
        "project-graph-query-python".to_string(),
    );
    env.insert(
        crate::native::runtime_hardening::STATE_DB_PATH_ENV.to_string(),
        state_db_path.to_string_lossy().to_string(),
    );
    env.insert("HIVEMIND_GRAPH_QUERY_PYTHON".to_string(), python_bin);
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "graph_query".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({
            "kind": "python_query",
            "repo_name": "repo",
            "max_results": 12,
            "limits": {
                "max_seconds": 2.0,
                "max_operations": 32,
                "max_trace_events": 2000,
                "max_stdout_chars": 512
            },
            "code": "files = graph.find(node_class='file', path_regex=r'src/.*', limit=10)\nhelpers = graph.find(node_class='symbol', name_regex='helper', path_regex=r'src/lib.rs', limit=1)\nhelper = helpers[0]\nsession.add(helper, detail='summary')\nsession.walk(helper, mode='dependents', depth=1, limit=8)\nresult = {'files': [node['logical_key'] for node in files]}"
        }),
    };

    let value = engine
        .execute(&action, &ctx)
        .expect("python graph query should run");
    let result: GraphQueryResult = serde_json::from_value(value).expect("decode result");
    assert_eq!(result.query_kind, "python_query");
    assert_eq!(result.python_repo_name.as_deref(), Some("repo"));
    assert!(!result.selected_block_ids.is_empty(), "{result:?}");
    assert!(
        result.nodes.iter().any(|node| {
            node.path.as_deref() == Some("src/lib.rs")
                || node.path.as_deref() == Some("src/main.rs")
        }),
        "{result:?}"
    );
    let files = result
        .python_result
        .as_ref()
        .and_then(|value| value.get("files"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(files.len() >= 2, "{result:?}");
    let operation_count = result
        .python_usage
        .as_ref()
        .and_then(|value| value.get("operation_count"))
        .and_then(Value::as_u64)
        .unwrap_or_default();
    assert!(operation_count > 0, "{result:?}");
    assert_eq!(
        sqlite_scalar_string(
            &state_db_path,
            "SELECT COUNT(1) FROM graphcode_sessions WHERE registry_key = 'project-graph-query-python:codegraph';",
        ),
        "1"
    );
}

#[test]
fn graph_query_and_checkpoint_contracts_have_extended_timeouts() {
    let engine = NativeToolEngine::default();
    let contracts = engine.contracts();

    let graph_query = contracts
        .iter()
        .find(|contract| contract.name == "graph_query")
        .expect("graph_query contract");
    assert_eq!(graph_query.timeout_ms, 15_000);

    let checkpoint_complete = contracts
        .iter()
        .find(|contract| contract.name == "checkpoint_complete")
        .expect("checkpoint_complete contract");
    assert_eq!(checkpoint_complete.timeout_ms, 20_000);
}

#[test]
fn execute_action_trace_truncates_large_responses_at_record_time() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");
    fs::write(repo.join("large.txt"), "A".repeat(20_000)).expect("write large file");

    let policy = NativeCommandPolicy::default();
    let scope = allow_all_scope();
    let env = HashMap::new();
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "read_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({"path": "large.txt"}),
    };

    let trace = engine.execute_action_trace("trace-large-read".to_string(), &action, &ctx);

    assert!(trace.response_truncated);
    assert!(
        trace.response_original_bytes.unwrap_or_default()
            > trace.response_stored_bytes.unwrap_or_default()
    );
    let response = trace.response.expect("response payload");
    assert!(response.contains("\"output_truncated\":true"));
}

#[test]
fn execute_action_trace_adds_rg_recovery_hint_for_grep_policy_failure() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy {
        allowlist: vec!["rg".to_string()],
        denylist: Vec::new(),
        deny_by_default: true,
    };
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "run_command".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "command": "grep", "args": ["-r", "Provenance", "src"] }),
    };

    let trace = engine.execute_action_trace("grep-policy".to_string(), &action, &ctx);
    let failure = trace.failure.expect("policy failure");

    assert_eq!(failure.code, "native_policy_violation");
    assert!(failure
        .recovery_hint
        .as_deref()
        .is_some_and(|hint| hint.contains("Use `rg` instead of `grep`")));
}

#[test]
fn execute_action_trace_adds_repo_relative_hint_for_invalid_path() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");

    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "list_files".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "path": "/tmp/not-the-repo/src/native", "recursive": false }),
    };

    let trace = engine.execute_action_trace("bad-path".to_string(), &action, &ctx);
    let failure = trace.failure.expect("path failure");

    assert_eq!(failure.code, "native_tool_input_invalid");
    assert!(failure
        .recovery_hint
        .as_deref()
        .is_some_and(|hint| hint.contains("repository-relative paths")));
}

#[test]
fn execute_action_trace_marks_missing_read_file_as_recoverable() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "read_file".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "path": "src/does-not-exist.rs" }),
    };

    let trace = engine.execute_action_trace("missing-read".to_string(), &action, &ctx);
    let failure = trace.failure.expect("missing file should fail");

    assert_eq!(failure.code, "native_tool_execution_failed");
    assert!(failure.recoverable);
    assert!(failure
        .recovery_hint
        .as_deref()
        .is_some_and(|hint| hint.contains("Check the repository-relative path first")));
}

proptest! {
    #[test]
    fn replay_is_deterministic_for_write_then_read(
        file_name in "[a-z]{1,8}",
        content in "[ -~]{0,64}"
    ) {
        let engine = NativeToolEngine::default();
        let scope = allow_all_scope();
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: vec!["rm".to_string()],
            deny_by_default: true,
        };

        let run_once = |root: &Path| -> (Value, Value) {
            let env = HashMap::new();
            let ctx = test_tool_context(root, Some(&scope), &policy, &env);
            let relative_path = format!("src/{file_name}.txt");
            let write = NativeToolAction {
                name: "write_file".to_string(),
                version: TOOL_VERSION_V1.to_string(),
                input: json!({"path": relative_path, "content": content}),
            };
            let read = NativeToolAction {
                name: "read_file".to_string(),
                version: TOOL_VERSION_V1.to_string(),
                input: json!({"path": format!("src/{file_name}.txt")}),
            };
            let write_out = engine.execute(&write, &ctx).expect("write must pass");
            let read_out = engine.execute(&read, &ctx).expect("read must pass");
            (write_out, read_out)
        };

        let tmp_a = tempfile::tempdir().expect("tempdir");
        let tmp_b = tempfile::tempdir().expect("tempdir");
        let first = run_once(tmp_a.path());
        let second = run_once(tmp_b.path());
        prop_assert_eq!(first, second);
    }
}

#[test]
fn dispatch_overhead_baseline_is_bounded() {
    let engine = NativeToolEngine::default();
    let tmp = tempfile::tempdir().expect("tempdir");
    fs::write(tmp.path().join("README.md"), "hello").expect("seed file");
    let policy = NativeCommandPolicy::default();
    let env = HashMap::new();
    let scope = allow_all_scope();
    let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
    let action = NativeToolAction {
        name: "list_files".to_string(),
        version: TOOL_VERSION_V1.to_string(),
        input: json!({ "path": ".", "recursive": false }),
    };

    let samples = 200_u32;
    let started = Instant::now();
    for _ in 0..samples {
        let _ = engine
            .execute(&action, &ctx)
            .expect("dispatch should succeed");
    }
    let avg_us = started.elapsed().as_micros() / u128::from(samples);
    assert!(
        avg_us < 50_000,
        "dispatch baseline too slow: average {avg_us}us"
    );
}

#[test]
#[ignore = "timeout in CI due to 4.5 minute runner limit"]
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
