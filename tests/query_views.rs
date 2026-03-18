mod support;

use support::*;

#[test]
fn cli_graph_query_filter_returns_bounded_results() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);
    std::fs::create_dir_all(repo_dir.join("src")).expect("mkdir src");
    std::fs::write(repo_dir.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
    std::fs::write(repo_dir.join("src/main.rs"), "fn main() { helper(); }\n").expect("write main");
    git_commit_all(&repo_dir, "add source files");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");
    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "snapshot", "refresh", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "graph",
            "query",
            "filter",
            "proj",
            "--type",
            "file",
            "--path",
            "src",
            "--max-results",
            "2",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let raw: serde_json::Value = serde_json::from_str(&out).expect("json output");
    let data = raw.get("data").unwrap_or(&raw);
    assert_eq!(
        data.get("query_kind").and_then(serde_json::Value::as_str),
        Some("filter"),
        "{out}"
    );
    assert_eq!(
        data.get("max_results").and_then(serde_json::Value::as_u64),
        Some(2),
        "{out}"
    );
    let nodes = data
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .expect("nodes array");
    assert!(nodes.len() <= 2, "{out}");
}

#[test]
fn cli_yaml_output_format() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, err) = run_hivemind(tmp.path(), &["-f", "yaml", "project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["-f", "yaml", "project", "list"]);
    assert_eq!(code, 0, "{err}");
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

    let (code, out, err) = run_hivemind(tmp.path(), &["attempt", "inspect", &t1_id]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("Task:") || out.contains("task_id"));

    let (code, _, _) = run_hivemind(
        tmp.path(),
        &["attempt", "inspect", "00000000-0000-0000-0000-000000000000"],
    );
    assert_ne!(code, 0);
}

#[test]
fn cli_exit_codes_for_not_found() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _, _) = run_hivemind(
        tmp.path(),
        &["task", "inspect", "00000000-0000-0000-0000-000000000000"],
    );
    assert_eq!(code, 2, "Expected exit code 2 for not_found");

    let (code, _, _) = run_hivemind(tmp.path(), &["project", "inspect", "nonexistent"]);
    assert_eq!(code, 2, "Expected exit code 2 for not_found");
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
