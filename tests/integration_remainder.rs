mod support;

use support::*;

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
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

    let (code, _out, err) = set_project_native_scripted_runtime(
        tmp.path(),
        "proj",
        &[
            "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"checkpoint done\"}",
            "DONE:runtime_ok",
        ],
        1000,
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

    let (code, worktree_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "worktree", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    let worktree_json: serde_json::Value =
        serde_json::from_str(&worktree_out).expect("worktree inspect json");
    let worktree_path = worktree_json
        .get("data")
        .and_then(|d| d.get("path"))
        .and_then(serde_json::Value::as_str)
        .expect("worktree path");
    let worktree_added = std::path::PathBuf::from(worktree_path).join("manual_diff_marker.txt");
    std::fs::write(&worktree_added, "changed\n").expect("write new worktree file");

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
    assert!(diff_out.contains("Attempt:"), "{diff_out}");
    assert!(diff_out.contains("Flow:"), "{diff_out}");
    assert!(diff_out.contains("Task:"), "{diff_out}");
    assert!(diff_out.contains("Changes:"), "{diff_out}");
}

#[test]
fn cli_task_commands_accept_legacy_project_task_arity() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "legacy-task"]);
    assert_eq!(code, 0, "{err}");
    let task_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "start", "proj", &task_id]);
    assert_ne!(
        code, 0,
        "legacy task start should fail because task is not in flow"
    );
    assert!(
        err.contains("task_not_in_flow"),
        "expected runtime validation error, got: {err}"
    );
    assert!(
        !err.contains("unexpected argument"),
        "legacy arity should parse without clap failure: {err}"
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "task",
            "complete",
            "proj",
            &task_id,
            "--success",
            "false",
            "--message",
            "legacy failure",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, inspect_out, err) = run_hivemind(tmp.path(), &["task", "inspect", &task_id]);
    assert_eq!(code, 0, "{err}");
    assert!(inspect_out.contains("State:       Closed"), "{inspect_out}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["task", "retry", "proj", &task_id, "--mode", "clean"],
    );
    assert_ne!(code, 0, "legacy task retry should be rejected");
    assert!(
        !err.contains("unexpected argument"),
        "legacy arity should parse without clap failure: {err}"
    );
}
