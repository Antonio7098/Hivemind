mod support;

use support::*;

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

    let (code, _, _) = run_hivemind(tmp.path(), &["merge", "prepare", &flow_id]);
    assert_ne!(code, 0);

    let (code, _, _) = run_hivemind(tmp.path(), &["merge", "approve", &flow_id]);
    assert_ne!(code, 0);
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
