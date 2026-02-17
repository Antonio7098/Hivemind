//! Integration tests for Hivemind.

use std::path::PathBuf;
use std::process::Command;

fn init_git_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir).expect("create repo dir");

    let out = Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .expect("git init");
    assert!(
        out.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");

    let out = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_dir)
        .output()
        .expect("git add");
    assert!(
        out.status.success(),
        "git add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Hivemind",
            "-c",
            "user.email=hivemind@example.com",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(repo_dir)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_project_governance_lifecycle_is_observable() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let legacy_constitution = repo_dir.join(".hivemind").join("constitution.yaml");
    let legacy_global_notepad = repo_dir.join(".hivemind").join("global").join("notepad.md");
    std::fs::create_dir_all(legacy_constitution.parent().expect("legacy parent")).expect("mkdir");
    std::fs::create_dir_all(legacy_global_notepad.parent().expect("legacy parent")).expect("mkdir");
    std::fs::write(&legacy_constitution, "legacy_constitution: true\n").expect("legacy file");
    std::fs::write(&legacy_global_notepad, "legacy notes\n").expect("legacy file");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "governance", "init", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, inspect_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "inspect", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value = serde_json::from_str(&inspect_out).expect("inspect json");
    let inspect_data = inspect_json.get("data").expect("inspect data");
    assert_eq!(
        inspect_data
            .get("initialized")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert!(
        inspect_data
            .get("artifacts")
            .and_then(|v| v.as_array())
            .is_some_and(|items| !items.is_empty()),
        "{inspect_out}"
    );

    let (code, migrate_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "migrate", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let migrate_json: serde_json::Value = serde_json::from_str(&migrate_out).expect("migrate json");
    let migrated_paths = migrate_json
        .get("data")
        .and_then(|v| v.get("migrated_paths"))
        .and_then(|v| v.as_array())
        .expect("migrated paths");
    assert!(
        migrated_paths.iter().any(|p| {
            p.as_str()
                .is_some_and(|s| s.contains("constitution.yaml") || s.contains("notepad.md"))
        }),
        "{migrate_out}"
    );

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "stream",
            "--project",
            "proj",
            "--limit",
            "400",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("governance_project_storage_initialized"),
        "{events_out}"
    );
    assert!(
        events_out.contains("governance_artifact_upserted"),
        "{events_out}"
    );
    assert!(
        events_out.contains("governance_storage_migrated"),
        "{events_out}"
    );
}

#[test]
fn cli_scope_violation_is_fatal_and_preserves_worktree() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "printf data > hm_scope_violation.txt; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let scope = r#"{"filesystem":{"rules":[{"pattern":"allowed","permission":"write"}]},"repositories":[],"git":{"permissions":[]},"execution":{"allowed":[],"denied":[]}}"#;
    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["task", "create", "proj", "t1", "--scope", scope],
    );
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    // Add a failing required check while the graph is still draft.
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "add-check",
            &graph_id,
            &t1_id,
            "--name",
            "fail_check",
            "--command",
            "exit 1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_ne!(code, 0, "expected fatal scope violation");
    assert!(
        err.contains("scope") || err.contains("scope_violation"),
        "{err}"
    );

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "events", "stream", "--flow", &flow_id],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("scope_violation_detected"),
        "{events_out}"
    );

    let (code, wt_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "worktree", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    assert!(wt_out.contains("\"is_worktree\": true"), "{wt_out}");
    assert!(
        wt_out.contains(worktree_root(tmp.path()).to_string_lossy().as_ref()),
        "{wt_out}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_sprint35_governance_artifacts_and_template_instantiation() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "governance", "init", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "doc1",
            "--title",
            "Architecture Notes",
            "--owner",
            "alice",
            "--tag",
            "architecture",
            "--content",
            "first revision",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "document",
            "update",
            "proj",
            "doc1",
            "--owner",
            "bob",
            "--content",
            "second revision",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, inspect_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "document",
            "inspect",
            "proj",
            "doc1",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value = serde_json::from_str(&inspect_out).expect("inspect json");
    let inspect_data = inspect_json.get("data").expect("inspect data");
    assert_eq!(
        inspect_data
            .get("latest_content")
            .and_then(serde_json::Value::as_str),
        Some("second revision")
    );
    assert_eq!(
        inspect_data
            .get("revisions")
            .and_then(serde_json::Value::as_array)
            .map(std::vec::Vec::len),
        Some(2)
    );
    assert_eq!(
        inspect_data
            .get("summary")
            .and_then(|s| s.get("owner"))
            .and_then(serde_json::Value::as_str),
        Some("bob")
    );
    assert!(
        inspect_data
            .get("summary")
            .and_then(|s| s.get("tags"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|tags| {
                tags.iter()
                    .any(|tag| tag.as_str().is_some_and(|value| value == "architecture"))
            }),
        "{inspect_out}"
    );

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let task_id = out
        .lines()
        .find_map(|line| {
            line.strip_prefix("ID:")
                .map(|value| value.trim().to_string())
        })
        .expect("task id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "attachment",
            "include",
            "proj",
            &task_id,
            "doc1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let project_notepad_marker = "PROJECT_NOTEPAD_SHOULD_NOT_APPEAR";
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "notepad",
            "create",
            "proj",
            "--content",
            project_notepad_marker,
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &task_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Graph ID:")
                .map(|value| value.trim().to_string())
        })
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Flow ID:")
                .map(|value| value.trim().to_string())
        })
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "400",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let runtime_prompt = events_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|events| {
            events.iter().find_map(|event| {
                let payload = event.get("payload")?;
                let typ = payload.get("type")?.as_str()?;
                if typ != "runtime_started" {
                    return None;
                }
                payload
                    .get("prompt")
                    .and_then(serde_json::Value::as_str)
                    .map(std::string::ToString::to_string)
            })
        })
        .expect("runtime prompt");
    assert!(
        runtime_prompt.contains("Execution attachments (explicit includes)"),
        "{runtime_prompt}"
    );
    assert!(
        runtime_prompt.contains("document_id: doc1"),
        "{runtime_prompt}"
    );
    assert!(
        runtime_prompt.contains("second revision"),
        "{runtime_prompt}"
    );
    assert!(
        !runtime_prompt.contains(project_notepad_marker),
        "project notepad content must not be injected by default: {runtime_prompt}"
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "skill",
            "create",
            "skill-a",
            "--name",
            "Skill A",
            "--content",
            "Do skill A",
            "--tag",
            "alpha",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "system-prompt",
            "create",
            "sp-main",
            "--content",
            "You are strict.",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "template",
            "create",
            "tpl-main",
            "--system-prompt-id",
            "sp-main",
            "--skill-id",
            "skill-a",
            "--document-id",
            "doc1",
            "--description",
            "Template body",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["global", "template", "instantiate", "proj", "tpl-main"],
    );
    assert_eq!(code, 0, "{err}");

    let (code, project_events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "stream",
            "--project",
            "proj",
            "--limit",
            "500",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        project_events_out.contains("template_instantiated"),
        "{project_events_out}"
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["global", "notepad", "create", "--content", "GLOBAL NOTE"],
    );
    assert_eq!(code, 0, "{err}");

    let (code, global_notepad_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "global", "notepad", "show"]);
    assert_eq!(code, 0, "{err}");
    let global_notepad_json: serde_json::Value =
        serde_json::from_str(&global_notepad_out).expect("global notepad json");
    let global_notepad_data = global_notepad_json
        .get("data")
        .expect("global notepad data");
    assert_eq!(
        global_notepad_data
            .get("content")
            .and_then(serde_json::Value::as_str),
        Some("GLOBAL NOTE")
    );
    assert_eq!(
        global_notepad_data
            .get("non_executional")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        global_notepad_data
            .get("non_validating")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["global", "notepad", "update", "--content", "GLOBAL NOTE V2"],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["global", "notepad", "delete"]);
    assert_eq!(code, 0, "{err}");

    let (code, global_notepad_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "global", "notepad", "show"]);
    assert_eq!(code, 0, "{err}");
    let global_notepad_json: serde_json::Value =
        serde_json::from_str(&global_notepad_out).expect("global notepad json");
    let global_notepad_data = global_notepad_json
        .get("data")
        .expect("global notepad data");
    assert_eq!(
        global_notepad_data
            .get("exists")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

fn hivemind_bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_hivemind").map_or_else(
        || {
            std::env::var("CARGO_BIN_EXE_hivemind")
                .map(PathBuf::from)
                .expect("CARGO_BIN_EXE_hivemind not set; build the hivemind binary")
        },
        PathBuf::from,
    )
}

fn run_hivemind(home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(hivemind_bin())
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run hivemind");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn worktree_root(home: &std::path::Path) -> PathBuf {
    home.join("hivemind").join("worktrees")
}

fn run_hivemind_with_env(
    home: &std::path::Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new(hivemind_bin());
    cmd.env("HOME", home).args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("run hivemind");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_verify_run_and_results_capture_check_outcomes() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out1, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out1
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "validate", &graph_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "add-check",
            &graph_id,
            &t1_id,
            "--name",
            "fail_check",
            "--command",
            "exit 1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 1, "expected verification failure; got: {err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "events", "stream", "--flow", &flow_id],
    );
    assert_eq!(code, 0, "{err}");
    assert!(events_out.contains("check_started"), "{events_out}");
    assert!(events_out.contains("check_completed"), "{events_out}");

    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let data = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("events data");
    let attempt_id = data
        .iter()
        .filter_map(|ev| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "check_completed" {
                return None;
            }
            payload
                .get("attempt_id")?
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .next_back()
        .expect("attempt id");

    let (code, results_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "verify", "results", &attempt_id],
    );
    assert_eq!(code, 0, "{err}");
    assert!(results_out.contains("fail_check"), "{results_out}");
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_verify_override_can_force_success_after_check_failure_and_is_audited() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out1, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out1
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "add-check",
            &graph_id,
            &t1_id,
            "--name",
            "fail_check",
            "--command",
            "exit 1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, _err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 1, "expected verification failure on required check");

    let reason = "manual override";
    let (code, _out, err) = run_hivemind_with_env(
        tmp.path(),
        &["verify", "override", &t1_id, "pass", "--reason", reason],
        &[("HIVEMIND_USER", "tester")],
    );
    assert_eq!(code, 0, "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "200",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let data = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("events data");

    let override_payload = data
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "human_override" {
                return None;
            }
            let task_id = payload.get("task_id")?.as_str()?;
            if task_id != t1_id {
                return None;
            }
            Some(payload)
        })
        .expect("expected human_override event");

    assert_eq!(
        override_payload.get("reason").and_then(|v| v.as_str()),
        Some(reason)
    );
    assert_eq!(
        override_payload.get("user").and_then(|v| v.as_str()),
        Some("tester")
    );

    let (code, flow_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "flow", "status", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let flow_json: serde_json::Value = serde_json::from_str(&flow_out).expect("flow json");
    assert_eq!(
        flow_json
            .get("success")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        flow_json
            .get("data")
            .and_then(|d| d.get("state"))
            .and_then(|s| s.as_str()),
        Some("completed")
    );
}

#[test]
fn cli_graph_flow_and_task_control_smoke() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out1, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out1
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, out2, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t2"]);
    assert_eq!(code, 0, "{err}");
    let t2_id = out2
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj",
            "g1",
            "--from-tasks",
            &t1_id,
            &t2_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["graph", "add-dependency", &graph_id, &t1_id, &t2_id],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "validate", &graph_id]);
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "json", "worktree", "list", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains(worktree_root(tmp.path()).to_string_lossy().as_ref()),
        "{out}"
    );

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "start", &t1_id]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "json", "worktree", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"is_worktree\": true"), "{out}");

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "worktree", "cleanup", &flow_id, "--force"],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"success\":true"), "{out}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "abort", &t1_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["task", "retry", &t1_id, "--reset-count", "--mode", "clean"],
    );
    assert_eq!(code, 0, "{err}");

    let (code, status_out, err) = run_hivemind(tmp.path(), &["flow", "status", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(status_out.contains("State:"));
}

#[test]
fn cli_task_retry_clean_resets_worktree_but_continue_preserves_it() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t2"]);
    assert_eq!(code, 0, "{err}");
    let t2_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj",
            "g1",
            "--from-tasks",
            &t1_id,
            &t2_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "validate", &graph_id]);
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "start", &t1_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "start", &t2_id]);
    assert_eq!(code, 0, "{err}");

    let t1_worktree = worktree_root(tmp.path()).join(&flow_id).join(&t1_id);
    let t2_worktree = worktree_root(tmp.path()).join(&flow_id).join(&t2_id);

    let t1_marker = t1_worktree.join("retry_marker.txt");
    let t2_marker = t2_worktree.join("retry_marker.txt");
    std::fs::write(&t1_marker, "x\n").expect("write marker");
    std::fs::write(&t2_marker, "x\n").expect("write marker");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "abort", &t1_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "abort", &t2_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) =
        run_hivemind(tmp.path(), &["task", "retry", &t1_id, "--mode", "continue"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        t1_marker.exists(),
        "continue retry must preserve worktree contents"
    );

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "retry", &t2_id, "--mode", "clean"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        !t2_marker.exists(),
        "clean retry must reset worktree contents"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn scheduler_emits_task_blocked_and_respects_dependency_order() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "\"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out1, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out1
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, out2, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t2"]);
    assert_eq!(code, 0, "{err}");
    let t2_id = out2
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj",
            "g1",
            "--from-tasks",
            &t1_id,
            &t2_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["graph", "add-dependency", &graph_id, &t1_id, &t2_id],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "validate", &graph_id]);
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "200",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let data = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("events data");

    let blocked_idx = data
        .iter()
        .enumerate()
        .find_map(|(idx, ev)| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "task_blocked" {
                return None;
            }
            let task_id = payload.get("task_id")?.as_str()?;
            if task_id != t2_id {
                return None;
            }
            Some(idx)
        })
        .expect("expected task_blocked for t2");

    let started_t1 = data
        .iter()
        .enumerate()
        .find_map(|(idx, ev)| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "task_execution_started" {
                return None;
            }
            let task_id = payload.get("task_id")?.as_str()?;
            if task_id != t1_id {
                return None;
            }
            Some(idx)
        })
        .expect("expected task_execution_started for t1");

    let started_t2 = data
        .iter()
        .enumerate()
        .find_map(|(idx, ev)| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "task_execution_started" {
                return None;
            }
            let task_id = payload.get("task_id")?.as_str()?;
            if task_id != t2_id {
                return None;
            }
            Some(idx)
        })
        .expect("expected task_execution_started for t2");

    assert!(
        blocked_idx < started_t1,
        "t1 should be blocked before it starts"
    );
    assert!(started_t1 < started_t2, "t1 must start before t2");
}

#[test]
fn cli_attempt_inspect_diff_after_manual_execution() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, start_out, err) = run_hivemind(tmp.path(), &["task", "start", &t1_id]);
    assert_eq!(code, 0, "{err}");
    let attempt_id = start_out
        .lines()
        .find_map(|l| l.strip_prefix("Attempt ID:").map(|s| s.trim().to_string()))
        .expect("attempt id");

    let worktree_readme = worktree_root(tmp.path())
        .join(&flow_id)
        .join(&t1_id)
        .join("README.md");
    std::fs::write(&worktree_readme, "changed\n").expect("modify worktree file");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "checkpoint",
            "complete",
            "--attempt-id",
            &attempt_id,
            "--id",
            "checkpoint-1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "complete", &t1_id]);
    assert_eq!(code, 0, "{err}");

    let (code, diff_out, err) =
        run_hivemind(tmp.path(), &["attempt", "inspect", &attempt_id, "--diff"]);
    assert_eq!(code, 0, "{err}");
    assert!(diff_out.contains("-test"), "{diff_out}");
    assert!(diff_out.contains("+changed"), "{diff_out}");
}

#[test]
fn cli_events_stream_with_filters() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    // Stream events filtered by flow
    let (code, out, err) = run_hivemind(tmp.path(), &["events", "stream", "--flow", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("flow_created") || out.contains("flow_started"));

    // Stream events filtered by project
    let (code, out, err) = run_hivemind(tmp.path(), &["events", "stream", "--project", "proj"]);
    assert_eq!(code, 0, "{err}");
    assert!(!out.contains("No events found."));

    // List events with flow + time range filters
    let (code, out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "list",
            "--flow",
            &flow_id,
            "--since",
            "1970-01-01T00:00:00Z",
            "--until",
            "2100-01-01T00:00:00Z",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("events list json");
    let events = json
        .get("data")
        .and_then(|v| v.as_array())
        .expect("events data array");
    assert!(!events.is_empty());

    // List with invalid timestamp should fail
    let (code, _out, _err) = run_hivemind(tmp.path(), &["events", "list", "--since", "not-a-time"]);
    assert_ne!(code, 0);

    // List with invalid range should fail
    let (code, _out, _err) = run_hivemind(
        tmp.path(),
        &[
            "events",
            "list",
            "--since",
            "2100-01-01T00:00:00Z",
            "--until",
            "1970-01-01T00:00:00Z",
        ],
    );
    assert_ne!(code, 0);

    // Stream with invalid flow ID
    let (code, _, _) = run_hivemind(tmp.path(), &["events", "stream", "--flow", "not-a-uuid"]);
    assert_ne!(code, 0);
}

#[test]
fn cli_events_replay_and_verify() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    // Replay without verify
    let (code, out, err) = run_hivemind(tmp.path(), &["events", "replay", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("State:"));

    // Replay with verify
    let (code, out, err) = run_hivemind(tmp.path(), &["events", "replay", &flow_id, "--verify"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Verification passed"));
}

#[test]
fn cli_yaml_output_format() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["-f", "yaml", "project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "yaml", "project", "list"]);
    assert_eq!(code, 0, "{err}");
    // YAML output should contain the project name
    assert!(out.contains("proj"));

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "json", "project", "list"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"proj\""));
}

#[test]
#[allow(clippy::similar_names)]
fn cli_graph_and_flow_list_support_project_filter() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, project_a_out, err) = run_hivemind(tmp.path(), &["project", "create", "proj-a"]);
    assert_eq!(code, 0, "{err}");
    let project_a_id = project_a_out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("project a id");

    let (code, project_b_out, err) = run_hivemind(tmp.path(), &["project", "create", "proj-b"]);
    assert_eq!(code, 0, "{err}");
    let project_b_id = project_b_out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("project b id");

    let (code, task_a_out, err) = run_hivemind(tmp.path(), &["task", "create", "proj-a", "ta"]);
    assert_eq!(code, 0, "{err}");
    let task_a_id = task_a_out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task a id");

    let (code, task_b_out, err) = run_hivemind(tmp.path(), &["task", "create", "proj-b", "tb"]);
    assert_eq!(code, 0, "{err}");
    let task_b_id = task_b_out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task b id");

    let (code, graph_a_out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj-a",
            "ga",
            "--from-tasks",
            &task_a_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_a_id = graph_a_out
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph a id");

    let (code, graph_b_out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj-b",
            "gb",
            "--from-tasks",
            &task_b_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_b_id = graph_b_out
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph b id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_a_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_b_id]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "graph", "list", "--project", "proj-a"],
    );
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("graph list json");
    assert_eq!(
        json.get("success").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    let graphs = json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("graph list data array");
    assert_eq!(graphs.len(), 1);
    assert_eq!(
        graphs[0].get("project_id").and_then(|v| v.as_str()),
        Some(project_a_id.as_str())
    );

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "flow", "list", "--project", "proj-b"],
    );
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("flow list json");
    assert_eq!(
        json.get("success").and_then(serde_json::Value::as_bool),
        Some(true)
    );
    let flows = json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("flow list data array");
    assert_eq!(flows.len(), 1);
    assert_eq!(
        flows[0].get("project_id").and_then(|v| v.as_str()),
        Some(project_b_id.as_str())
    );
}

#[test]
fn cli_attempt_inspect() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    // Inspect attempt using task ID
    let (code, out, err) = run_hivemind(tmp.path(), &["attempt", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Task:") || out.contains("task_id"));

    // Inspect non-existent attempt
    let (code, _, _) = run_hivemind(
        tmp.path(),
        &["attempt", "inspect", "00000000-0000-0000-0000-000000000000"],
    );
    assert_ne!(code, 0);
}

#[test]
fn cli_exit_codes_for_not_found() {
    let tmp = tempfile::tempdir().expect("tempdir");

    // Not found should return exit code 2
    let (code, _, _) = run_hivemind(
        tmp.path(),
        &["task", "inspect", "00000000-0000-0000-0000-000000000000"],
    );
    assert_eq!(code, 2, "Expected exit code 2 for not_found");

    let (code, _, _) = run_hivemind(tmp.path(), &["project", "inspect", "nonexistent"]);
    assert_eq!(code, 2, "Expected exit code 2 for not_found");
}

#[test]
fn cli_merge_lifecycle() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    // merge prepare should fail on non-completed flow
    let (code, _, _) = run_hivemind(tmp.path(), &["merge", "prepare", &flow_id]);
    assert_ne!(code, 0);

    // merge approve should fail without prepare
    let (code, _, _) = run_hivemind(tmp.path(), &["merge", "approve", &flow_id]);
    assert_ne!(code, 0);
}

#[test]
fn cli_runtime_config_and_flow_tick() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--model",
            "test-provider/test-model",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo '$ cargo test'; echo 'Tool: grep'; echo '- [ ] collect logs'; echo '- [x] collect logs'; echo 'I will verify outputs'; echo stderr_line 1>&2; printf data > hm_sprint14.txt; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "events", "stream", "--flow", &flow_id],
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("runtime_started"), "{out}");
    assert!(out.contains("runtime_output_chunk"), "{out}");
    assert!(
        out.contains("runtime_exited") || out.contains("runtime_terminated"),
        "{out}"
    );
    assert!(out.contains("runtime_filesystem_observed"), "{out}");
    assert!(out.contains("runtime_command_observed"), "{out}");
    assert!(out.contains("runtime_tool_call_observed"), "{out}");
    assert!(out.contains("runtime_todo_snapshot_updated"), "{out}");
    assert!(out.contains("runtime_narrative_output_observed"), "{out}");

    let events_json: serde_json::Value = serde_json::from_str(&out).expect("events json");
    let data = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("events data");
    let runtime_started = data
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "runtime_started" {
                return None;
            }
            Some(payload)
        })
        .expect("runtime_started payload");

    let prompt = runtime_started
        .get("prompt")
        .and_then(|v| v.as_str())
        .expect("runtime prompt");
    assert!(prompt.contains("Task:"));
    assert!(prompt.contains("Success Criteria:"));

    let flags = runtime_started
        .get("flags")
        .and_then(|v| v.as_array())
        .expect("runtime flags");
    assert_eq!(flags.first().and_then(|v| v.as_str()), Some("sh"));
    assert_eq!(flags.get(1).and_then(|v| v.as_str()), Some("-c"));
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_checkpoint_complete_unblocks_attempt_and_emits_lifecycle_events() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo runtime_ok",
            "--timeout-ms",
            "1000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("checkpoints_incomplete"), "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "200",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(events_out.contains("checkpoint_declared"), "{events_out}");
    assert!(events_out.contains("checkpoint_activated"), "{events_out}");

    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let data = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .expect("events data");
    let attempt_id = data
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            let typ = payload.get("type")?.as_str()?;
            if typ != "attempt_started" {
                return None;
            }
            payload
                .get("attempt_id")?
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .expect("attempt id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "checkpoint",
            "complete",
            "--attempt-id",
            &attempt_id,
            "--id",
            "checkpoint-1",
            "--summary",
            "checkpoint done",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "300",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(events_out.contains("checkpoint_completed"), "{events_out}");
    assert!(
        events_out.contains("all_checkpoints_completed"),
        "{events_out}"
    );
    assert!(
        events_out.contains("checkpoint_commit_created"),
        "{events_out}"
    );
}

#[test]
fn cli_scope_violation_detects_tmp_write_outside_worktree() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "tmp=\"/tmp/hm_scope_${HIVEMIND_ATTEMPT_ID}.txt\"; printf data > \"$tmp\"; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "2000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let scope = r#"{"filesystem":{"rules":[{"pattern":"allowed/**","permission":"write"}]},"repositories":[],"git":{"permissions":[]},"execution":{"allowed":[],"denied":[]}}"#;
    let (code, out, err) = run_hivemind(
        tmp.path(),
        &["task", "create", "proj", "t1", "--scope", scope],
    );
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_ne!(code, 0, "expected scope violation");
    assert!(err.contains("scope"), "{err}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "200",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("scope_violation_detected"),
        "{events_out}"
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn cli_attempt_inspect_context_returns_retry_context() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "runtime-set",
            "proj",
            "--binary-path",
            "/usr/bin/env",
            "--arg",
            "sh",
            "--arg",
            "-c",
            "--arg",
            "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
            "--timeout-ms",
            "2000",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "add-check",
            &graph_id,
            &t1_id,
            "--name",
            "fail_check",
            "--command",
            "exit 1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, _err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 1);
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["task", "retry", &t1_id, "--mode", "continue"]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, _err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert_eq!(code, 1);

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "300",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let attempt_id = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|ev| {
                    let payload = ev.get("payload")?;
                    let typ = payload.get("type")?.as_str()?;
                    if typ != "attempt_started" {
                        return None;
                    }
                    payload
                        .get("attempt_id")
                        .and_then(serde_json::Value::as_str)
                        .map(std::string::ToString::to_string)
                })
                .next_back()
        })
        .expect("attempt id");

    let (code, inspect_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "attempt", "inspect", &attempt_id, "--context"],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value =
        serde_json::from_str(&inspect_out).expect("attempt inspect json");
    let context = inspect_json
        .get("context")
        .and_then(serde_json::Value::as_str)
        .expect("context in attempt inspect");
    assert!(context.contains("Retry attempt 2/"), "{context}");
}

#[test]
fn cli_worktree_cleanup_requires_force_on_running_flow() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, _err) = run_hivemind(tmp.path(), &["worktree", "cleanup", &flow_id]);
    assert_eq!(code, 3);
    let (code, out, err) = run_hivemind(tmp.path(), &["worktree", "cleanup", &flow_id, "--force"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Cleanup complete."), "{out}");

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "100",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("worktree_cleanup_performed"),
        "{events_out}"
    );
}

#[test]
fn cli_flow_restart_creates_replacement_for_aborted_flow() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "abort", &flow_id, "--force"]);
    assert_eq!(code, 0, "{err}");

    let (code, restart_out, err) = run_hivemind(tmp.path(), &["flow", "restart", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let restarted_flow_id = restart_out
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("restarted flow id");
    assert_ne!(restarted_flow_id, flow_id);

    let (code, status_out, err) = run_hivemind(tmp.path(), &["flow", "status", &restarted_flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(status_out.contains("State:   Created"), "{status_out}");
}

#[test]
fn cli_attempt_list_and_checkpoint_list_show_attempt_progress() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let task_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &task_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, start_out, err) = run_hivemind(tmp.path(), &["task", "start", &task_id]);
    assert_eq!(code, 0, "{err}");
    let attempt_id = start_out
        .lines()
        .find_map(|l| l.strip_prefix("Attempt ID:").map(|s| s.trim().to_string()))
        .expect("attempt id");

    let (code, attempts_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "attempt", "list", "--flow", &flow_id, "--limit", "20",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let attempts_json: serde_json::Value =
        serde_json::from_str(&attempts_out).expect("attempt list json");
    let attempts = attempts_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("attempt list array");
    assert!(attempts.iter().any(|attempt| {
        attempt
            .get("attempt_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == attempt_id)
    }));

    let (code, checkpoints_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "checkpoint", "list", &attempt_id],
    );
    assert_eq!(code, 0, "{err}");
    let checkpoints_json: serde_json::Value =
        serde_json::from_str(&checkpoints_out).expect("checkpoint list json");
    let checkpoints = checkpoints_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("checkpoint list array");
    assert!(checkpoints.iter().any(|checkpoint| {
        checkpoint
            .get("checkpoint_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == "checkpoint-1")
    }));
}

#[test]
fn cli_abort_flow_transitions_running_tasks_to_failed() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "start", &t1_id]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "abort", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, status_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "flow", "status", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let status_json: serde_json::Value = serde_json::from_str(&status_out).expect("flow status");
    let task_state = status_json
        .get("data")
        .and_then(|d| d.get("task_executions"))
        .and_then(|d| d.get(&t1_id))
        .and_then(|d| d.get("state"))
        .and_then(serde_json::Value::as_str);
    assert_eq!(task_state, Some("failed"));
}

#[test]
fn cli_merge_prepare_blocked_emits_error_event() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g1", "--from-tasks", &t1_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, _err) = run_hivemind(tmp.path(), &["merge", "prepare", &flow_id]);
    assert_ne!(code, 0);

    let (code, events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "list", "--flow", &flow_id, "--limit", "100",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let events_json: serde_json::Value = serde_json::from_str(&events_out).expect("events json");
    let has_blocked_error = events_json
        .get("data")
        .and_then(|d| d.as_array())
        .is_some_and(|arr| {
            arr.iter().any(|ev| {
                ev.get("payload")
                    .and_then(|p| p.get("type"))
                    .and_then(serde_json::Value::as_str)
                    == Some("error_occurred")
                    && ev
                        .get("payload")
                        .and_then(|p| p.get("error"))
                        .and_then(|e| e.get("code"))
                        .and_then(serde_json::Value::as_str)
                        == Some("flow_not_completed")
            })
        });
    assert!(
        has_blocked_error,
        "expected error_occurred(flow_not_completed) in events: {events_out}"
    );
}

#[test]
fn cli_dependency_chain_only_root_task_starts_ready() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let (code, out1, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");
    let t1_id = out1
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, out2, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t2"]);
    assert_eq!(code, 0, "{err}");
    let t2_id = out2
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, out3, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t3"]);
    assert_eq!(code, 0, "{err}");
    let t3_id = out3
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, gout, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "create",
            "proj",
            "g1",
            "--from-tasks",
            &t1_id,
            &t2_id,
            &t3_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let graph_id = gout
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["graph", "add-dependency", &graph_id, &t1_id, &t2_id],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["graph", "add-dependency", &graph_id, &t2_id, &t3_id],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "validate", &graph_id]);
    assert_eq!(code, 0, "{err}");

    let (code, fout, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_id]);
    assert_eq!(code, 0, "{err}");
    let flow_id = fout
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_id]);
    assert_eq!(code, 0, "{err}");

    let (code, status_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "flow", "status", &flow_id]);
    assert_eq!(code, 0, "{err}");
    let status_json: serde_json::Value = serde_json::from_str(&status_out).expect("flow status");

    let task_executions = status_json
        .get("data")
        .and_then(|d| d.get("task_executions"))
        .and_then(serde_json::Value::as_object)
        .expect("task executions object");

    let t1_state = task_executions
        .get(&t1_id)
        .and_then(|v| v.get("state"))
        .and_then(serde_json::Value::as_str);
    let t2_state = task_executions
        .get(&t2_id)
        .and_then(|v| v.get("state"))
        .and_then(serde_json::Value::as_str);
    let t3_state = task_executions
        .get(&t3_id)
        .and_then(|v| v.get("state"))
        .and_then(serde_json::Value::as_str);

    assert_eq!(t1_state, Some("ready"));
    assert_eq!(t2_state, Some("pending"));
    assert_eq!(t3_state, Some("pending"));
}
