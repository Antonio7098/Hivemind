use super::*;
use crate::core::scope::{
    ExecutionScope, FilePermission, FilesystemScope, PathRule, RepositoryScope, Scope,
};
use proptest::prelude::*;
use std::sync::{Mutex, MutexGuard, OnceLock};
use ucp_api::{
    build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig, CODEGRAPH_EXTRACTOR_VERSION,
};

mod network;
mod sessions;

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
    let mut env = HashMap::new();
    env.insert(
        GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
        snapshot_path.to_string_lossy().to_string(),
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
