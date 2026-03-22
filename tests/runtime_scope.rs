mod support;

use std::process::Command;
use support::*;

fn strace_functional() -> bool {
    if cfg!(windows) {
        return false;
    }
    // Test if strace can actually capture syscalls
    let output = Command::new("strace")
        .args(["-o", "/dev/null", "-e", "trace=file", "--", "true"])
        .output();
    matches!(output, Ok(o) if o.status.success())
}

#[test]
fn cli_scope_violation_is_fatal_and_preserves_worktree() {
    if cfg!(windows) || !strace_functional() {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");
    let unix_runtime_script = "printf data > hm_scope_violation.txt; printf data > \"$HOME/hm_scope_scope_violation.txt\"".to_string();
    let windows_runtime_script =
        "echo data > hm_scope_violation.txt & echo data > \"%USERPROFILE%\\hm_scope_scope_violation.txt\""
            .to_string();

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        &unix_runtime_script,
        &windows_runtime_script,
        3000,
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
            failing_check_command(),
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
        err.contains("scope")
            || err.contains("scope_violation")
            || err.contains("checkpoints_incomplete"),
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

    let unix_runtime_script = "echo '$ cargo test'; echo 'Tool: grep'; echo '- [ ] collect logs'; echo '- [x] collect logs'; echo 'I will verify outputs'; echo stderr_line 1>&2; printf data > hm_sprint14.txt";
    let windows_runtime_script = "echo ^$ cargo test & echo Tool: grep & echo - [ ] collect logs & echo - [x] collect logs & echo I will verify outputs & echo stderr_line 1>&2 & echo data > hm_sprint14.txt";
    let (code, _out, err) = set_project_runtime_script_with_model(
        tmp.path(),
        "proj",
        Some("test-provider/test-model"),
        unix_runtime_script,
        windows_runtime_script,
        1000,
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
    assert!(
        code == 0 || code == 1,
        "expected runtime tick to succeed or surface incomplete execution; got code {code}: {err}"
    );

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
    let expected_flags = expected_runtime_flag_prefix();
    for (idx, expected) in expected_flags.iter().enumerate() {
        assert_eq!(flags.get(idx).and_then(|v| v.as_str()), Some(*expected));
    }
}

#[test]
fn cli_scope_violation_detects_tmp_write_outside_worktree() {
    if cfg!(windows) || !strace_functional() {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");
    let unix_runtime_script = "printf data > \"$HOME/hm_scope_tmp_violation.txt\"".to_string();
    let windows_runtime_script =
        "echo data > \"%USERPROFILE%\\hm_scope_tmp_violation.txt\"".to_string();

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        &unix_runtime_script,
        &windows_runtime_script,
        2000,
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
    assert!(
        err.contains("scope") || err.contains("checkpoints_incomplete"),
        "{err}"
    );

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
