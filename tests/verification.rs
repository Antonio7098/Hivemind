mod support;

use support::*;

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
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

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert!(
        code == 0 || code == 1,
        "expected tick to complete or surface verification failure; got code {code}: {err}"
    );

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
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
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

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        "echo runtime_ok",
        "echo runtime_ok",
        1000,
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

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert!(
        code == 0 || code == 1,
        "expected tick to complete or surface verification failure; got code {code}: {err}"
    );

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
    let _attempt_id = data
        .iter()
        .filter_map(|ev| {
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
        .next_back()
        .expect("attempt id");

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
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
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

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        "echo runtime_ok",
        "echo runtime_ok",
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
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_attempt_inspect_context_returns_manifest_and_retry_linkage() {
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

    let constitution_yaml = "version: 1\nschema_version: constitution.v1\ncompatibility:\n  minimum_hivemind_version: 0.1.0\n  governance_schema_version: governance.v1\npartitions: []\nrules: []";
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "constitution",
            "init",
            "proj",
            "--content",
            constitution_yaml,
            "--confirm",
        ],
    );
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
            "Doc One",
            "--owner",
            "owner",
            "--content",
            "document-one",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "doc2",
            "--title",
            "Doc Two",
            "--owner",
            "owner",
            "--content",
            "document-two",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "system-prompt",
            "create",
            "sp1",
            "--content",
            "Always follow project governance artifacts.",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "skill",
            "create",
            "skill1",
            "--name",
            "Skill One",
            "--content",
            "Use deterministic edits.",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "template",
            "create",
            "tpl1",
            "--system-prompt-id",
            "sp1",
            "--skill-id",
            "skill1",
            "--document-id",
            "doc1",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["global", "template", "instantiate", "proj", "tpl1"],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = set_project_native_scripted_runtime(
        tmp.path(),
        "proj",
        &[
            "ACT:tool:checkpoint_complete:{\"id\":\"checkpoint-1\",\"summary\":\"checkpoint done\"}",
            "DONE:runtime_ok",
        ],
        2000,
    );
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

    for task_id in [&t1_id, &t2_id] {
        let (code, _out, err) = run_hivemind(
            tmp.path(),
            &[
                "project",
                "governance",
                "attachment",
                "include",
                "proj",
                task_id,
                "doc2",
            ],
        );
        assert_eq!(code, 0, "{err}");
        let (code, _out, err) = run_hivemind(
            tmp.path(),
            &[
                "project",
                "governance",
                "attachment",
                "exclude",
                "proj",
                task_id,
                "doc1",
            ],
        );
        assert_eq!(code, 0, "{err}");
    }

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

    let (code, graph_two_output, err) = run_hivemind(
        tmp.path(),
        &["graph", "create", "proj", "g2", "--from-tasks", &t2_id],
    );
    assert_eq!(code, 0, "{err}");
    let graph_two_id = graph_two_output
        .lines()
        .find_map(|l| l.strip_prefix("Graph ID:").map(|s| s.trim().to_string()))
        .expect("graph id");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "graph",
            "add-check",
            &graph_two_id,
            &t2_id,
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
    let (code, flow_two_output, err) = run_hivemind(tmp.path(), &["flow", "create", &graph_two_id]);
    assert_eq!(code, 0, "{err}");
    let flow_two_id = flow_two_output
        .lines()
        .find_map(|l| l.strip_prefix("Flow ID:").map(|s| s.trim().to_string()))
        .expect("flow id");
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "start", &flow_two_id]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_id]);
    assert!(
        code == 0 || code == 1,
        "expected tick to complete or surface verification failure; got code {code}: {err}"
    );
    let (code, _out, err) = run_hivemind(tmp.path(), &["flow", "tick", &flow_two_id]);
    assert!(
        code == 0 || code == 1,
        "expected tick to complete or surface verification failure; got code {code}: {err}"
    );

    let (code, flow1_events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f", "json", "events", "stream", "--flow", &flow_id, "--limit", "300",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let flow1_events_json: serde_json::Value =
        serde_json::from_str(&flow1_events_out).expect("flow1 events json");
    let flow1_events = flow1_events_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("flow1 events array");
    let first_assembled = flow1_events
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            if payload.get("type")?.as_str()? != "attempt_context_assembled" {
                return None;
            }
            Some(payload.clone())
        })
        .expect("first assembled context event");
    let first_manifest_hash = first_assembled
        .get("manifest_hash")
        .and_then(serde_json::Value::as_str)
        .expect("manifest hash")
        .to_string();
    let first_inputs_hash = first_assembled
        .get("inputs_hash")
        .and_then(serde_json::Value::as_str)
        .expect("inputs hash")
        .to_string();
    let first_window_hash = flow1_events
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            if payload.get("type")?.as_str()? != "context_window_created" {
                return None;
            }
            payload
                .get("state_hash")
                .and_then(serde_json::Value::as_str)
                .map(std::string::ToString::to_string)
        })
        .expect("context window hash");

    let (code, flow2_events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "stream",
            "--flow",
            &flow_two_id,
            "--limit",
            "300",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let flow2_events_json: serde_json::Value =
        serde_json::from_str(&flow2_events_out).expect("flow2 events json");
    let flow2_events = flow2_events_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("flow2 events array");
    let second_inputs_hash = flow2_events
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            if payload.get("type")?.as_str()? != "attempt_context_assembled" {
                return None;
            }
            payload
                .get("inputs_hash")
                .and_then(serde_json::Value::as_str)
                .map(std::string::ToString::to_string)
        })
        .expect("second inputs hash");
    assert_eq!(first_inputs_hash, second_inputs_hash);
    let second_window_hash = flow2_events
        .iter()
        .find_map(|ev| {
            let payload = ev.get("payload")?;
            if payload.get("type")?.as_str()? != "context_window_created" {
                return None;
            }
            payload
                .get("state_hash")
                .and_then(serde_json::Value::as_str)
                .map(std::string::ToString::to_string)
        })
        .expect("second context window hash");
    assert_eq!(first_window_hash, second_window_hash);

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
        .and_then(serde_json::Value::as_object)
        .expect("context in attempt inspect");
    let retry = context
        .get("retry")
        .and_then(serde_json::Value::as_str)
        .expect("retry context");
    assert!(retry.contains("Retry attempt 2/"), "{retry}");

    let manifest = context.get("manifest").expect("manifest");
    assert_eq!(
        manifest
            .get("manifest_version")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    let ordered_inputs = manifest
        .get("ordered_inputs")
        .and_then(serde_json::Value::as_array)
        .expect("ordered inputs");
    let ordered_inputs: Vec<String> = ordered_inputs
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(std::string::ToString::to_string)
        .collect();
    assert_eq!(
        ordered_inputs,
        vec![
            "constitution".to_string(),
            "system_prompt".to_string(),
            "skills".to_string(),
            "project_documents".to_string(),
            "graph_summary".to_string()
        ]
    );

    let excluded_sources = manifest
        .get("excluded_sources")
        .and_then(serde_json::Value::as_array)
        .expect("excluded sources");
    assert!(excluded_sources
        .iter()
        .any(|v| v.as_str() == Some("project_notepad")));
    assert!(excluded_sources
        .iter()
        .any(|v| v.as_str() == Some("global_notepad")));
    assert!(excluded_sources
        .iter()
        .any(|v| v.as_str() == Some("implicit_memory")));

    assert_eq!(
        manifest
            .get("template_id")
            .and_then(serde_json::Value::as_str),
        Some("tpl1")
    );
    let documents = manifest
        .get("documents")
        .and_then(serde_json::Value::as_array)
        .expect("documents");
    assert!(documents.iter().any(|item| {
        item.get("document_id").and_then(serde_json::Value::as_str) == Some("doc2")
    }));
    assert!(!documents.iter().any(|item| {
        item.get("document_id").and_then(serde_json::Value::as_str) == Some("doc1")
    }));

    let retry_links = manifest
        .get("retry_links")
        .and_then(serde_json::Value::as_array)
        .expect("retry links");
    assert_eq!(retry_links.len(), 1);
    assert_eq!(
        retry_links[0]
            .get("manifest_hash")
            .and_then(serde_json::Value::as_str),
        Some(first_manifest_hash.as_str())
    );

    assert!(
        context
            .get("inputs_hash")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "{inspect_out}"
    );
    assert_eq!(
        context
            .get("context_window_hash")
            .and_then(serde_json::Value::as_str),
        Some(first_window_hash.as_str())
    );
    assert!(
        context
            .get("rendered_prompt_hash")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "{inspect_out}"
    );
}
