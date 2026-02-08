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
