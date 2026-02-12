//! Integration tests for Hivemind.

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
            "printf data > hm_scope_violation.txt",
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
    assert!(wt_out.contains(".hivemind/worktrees"), "{wt_out}");
}

fn run_hivemind(home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_hivemind"))
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

fn run_hivemind_with_env(
    home: &std::path::Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_hivemind"));
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
            "echo runtime_ok",
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
            "echo runtime_ok",
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
        flow_json.get("state").and_then(|s| s.as_str()),
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
    assert!(out.contains(".hivemind/worktrees"), "{out}");

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "json", "worktree", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"is_worktree\": true"), "{out}");

    let (code, out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "worktree", "cleanup", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"success\":true"), "{out}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "abort", &t1_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "retry", &t1_id, "--reset-count"]);
    assert_eq!(code, 0, "{err}");

    let (code, status_out, err) = run_hivemind(tmp.path(), &["flow", "status", &flow_id]);
    assert_eq!(code, 0, "{err}");
    assert!(status_out.contains("State:"));
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
            "exit 0",
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

    let worktree_readme = repo_dir
        .join(".hivemind/worktrees")
        .join(&flow_id)
        .join(&t1_id)
        .join("README.md");
    std::fs::write(&worktree_readme, "changed\n").expect("modify worktree file");

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
            "echo stdout_line; echo stderr_line 1>&2; printf data > hm_phase14.txt",
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
}
