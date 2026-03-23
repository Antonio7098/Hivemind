//! Event CLI integration tests for Hivemind.

use std::path::PathBuf;

mod support;

use support::*;

#[test]
fn cli_events_list_supports_error_type_filter() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "err-task"]);
    assert_eq!(code, 0, "{err}");
    let task_id = out
        .lines()
        .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
        .expect("task id");

    let (code, _out, _err) = run_hivemind(tmp.path(), &["task", "start", &task_id]);
    assert_ne!(
        code, 0,
        "starting a task outside a flow should fail and emit user error event"
    );

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "list",
            "--project",
            "proj",
            "--error-type",
            "user",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let json: serde_json::Value = serde_json::from_str(&out).expect("events list json");
    let events = json
        .get("data")
        .and_then(|v| v.as_array())
        .expect("events data array");
    assert!(!events.is_empty(), "expected at least one user error event");
    assert!(events.iter().all(|event| {
        event
            .get("payload")
            .and_then(|v| v.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("error_occurred")
    }));
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
fn cli_events_verify_and_recover_from_mirror_restores_canonical_db() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", "t1"]);
    assert_eq!(code, 0, "{err}");

    let db_path = tmp.path().join(".hivemind").join("db.sqlite");
    if db_path.exists() {
        std::fs::remove_file(&db_path).expect("remove sqlite db");
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", db_path.to_string_lossy(), suffix));
        if sidecar.exists() {
            std::fs::remove_file(sidecar).expect("remove sqlite sidecar");
        }
    }

    let (code, verify_out, err) = run_hivemind(tmp.path(), &["-f", "json", "events", "verify"]);
    assert_eq!(code, 0, "{err}");
    let verify_json: serde_json::Value = serde_json::from_str(&verify_out).expect("verify json");
    assert_eq!(
        verify_json
            .get("data")
            .and_then(|d| d.get("parity_ok"))
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "{verify_out}"
    );
    assert_eq!(
        verify_json
            .get("data")
            .and_then(|d| d.get("sqlite"))
            .and_then(|d| d.get("event_count"))
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "{verify_out}"
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "events", "recover", "--from-mirror"],
    );
    assert_ne!(
        code, 0,
        "recover should require explicit confirmation: {err}"
    );

    let (code, recover_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "recover",
            "--from-mirror",
            "--confirm",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let recover_json: serde_json::Value = serde_json::from_str(&recover_out).expect("recover json");
    assert_eq!(
        recover_json
            .get("data")
            .and_then(|d| d.get("verification"))
            .and_then(|d| d.get("parity_ok"))
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "{recover_out}"
    );

    let (code, list_out, err) = run_hivemind(tmp.path(), &["-f", "json", "project", "list"]);
    assert_eq!(code, 0, "{err}");
    let list_json: serde_json::Value = serde_json::from_str(&list_out).expect("project list json");
    let projects = list_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("project list array");
    assert!(
        projects.iter().any(|project| {
            project.get("name").and_then(serde_json::Value::as_str) == Some("proj")
        }),
        "{list_out}"
    );
}
