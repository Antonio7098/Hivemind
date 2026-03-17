mod support;

use support::*;

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
