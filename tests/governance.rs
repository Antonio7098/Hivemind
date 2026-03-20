//! Governance-focused integration tests for Hivemind.

use std::process::Command;

mod support;

use support::*;

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
fn cli_project_governance_init_accepts_project_flag() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "init",
            "--project",
            "proj",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("init json");
    assert_eq!(
        json.get("success").and_then(serde_json::Value::as_bool),
        Some(true),
        "{out}"
    );
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
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

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
        "echo runtime_ok & \"%HIVEMIND_BIN%\" checkpoint complete --id checkpoint-1",
        1000,
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
    assert!(runtime_prompt.contains("Documents:"), "{runtime_prompt}");
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

    let (code, template_events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "list",
            "--project",
            "proj",
            "--template-id",
            "tpl-main",
            "--limit",
            "200",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let template_events_json: serde_json::Value =
        serde_json::from_str(&template_events_out).expect("template events json");
    let template_events = template_events_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("template events array");
    assert!(template_events.iter().any(|event| {
        event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("template_instantiated")
    }));

    let (code, artifact_events_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "list",
            "--project",
            "proj",
            "--artifact-id",
            "doc1",
            "--limit",
            "200",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let artifact_events_json: serde_json::Value =
        serde_json::from_str(&artifact_events_out).expect("artifact events json");
    let artifact_events = artifact_events_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("artifact events array");
    assert!(!artifact_events.is_empty(), "{artifact_events_out}");
    assert!(artifact_events.iter().any(|event| {
        event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|typ| {
                typ == "template_instantiated" || typ == "governance_attachment_lifecycle_updated"
            })
    }));

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

    let (code, replay_out, err) =
        run_hivemind(tmp.path(), &["events", "replay", &flow_id, "--verify"]);
    assert_eq!(code, 0, "{err}");
    assert!(replay_out.contains("Verification passed"), "{replay_out}");

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

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_sprint36_constitution_lifecycle_and_auditability() {
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

    let constitution_path = tmp.path().join("constitution.yaml");
    std::fs::write(
        &constitution_path,
        r"version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.28
  governance_schema_version: governance.v1
partitions:
  - id: domain
    path: src/domain
  - id: infrastructure
    path: src/infrastructure
rules:
  - type: forbidden_dependency
    id: no_domain_to_infra
    from: domain
    to: infrastructure
    severity: hard
",
    )
    .expect("write constitution");

    let constitution_path_arg = constitution_path.to_string_lossy().to_string();
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "constitution",
            "init",
            "proj",
            "--from-file",
            &constitution_path_arg,
        ],
    );
    assert_ne!(code, 0, "expected confirmation-required failure");
    assert!(
        err.contains("constitution_confirmation_required") || err.contains("--confirm"),
        "{err}"
    );

    let (code, init_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "constitution",
            "init",
            "proj",
            "--from-file",
            &constitution_path_arg,
            "--confirm",
            "--actor",
            "tester",
            "--intent",
            "bootstrap constitution",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let init_json: serde_json::Value = serde_json::from_str(&init_out).expect("init json");
    assert_eq!(
        init_json
            .get("data")
            .and_then(|v| v.get("confirmed"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        init_json
            .get("data")
            .and_then(|v| v.get("actor"))
            .and_then(serde_json::Value::as_str),
        Some("tester")
    );

    let (code, show_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "constitution", "show", "proj"]);
    assert_eq!(code, 0, "{err}");
    let show_json: serde_json::Value = serde_json::from_str(&show_out).expect("show json");
    let show_data = show_json.get("data").expect("show data");
    assert_eq!(
        show_data
            .get("schema_version")
            .and_then(serde_json::Value::as_str),
        Some("constitution.v1")
    );
    assert_eq!(
        show_data
            .get("constitution_version")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        show_data
            .get("partitions")
            .and_then(serde_json::Value::as_array)
            .map(std::vec::Vec::len),
        Some(2)
    );
    assert_eq!(
        show_data
            .get("rules")
            .and_then(serde_json::Value::as_array)
            .map(std::vec::Vec::len),
        Some(1)
    );

    let (code, validate_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "constitution", "validate", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let validate_json: serde_json::Value =
        serde_json::from_str(&validate_out).expect("validate json");
    assert_eq!(
        validate_json
            .get("data")
            .and_then(|v| v.get("valid"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let bad_constitution_path = tmp.path().join("constitution-bad.yaml");
    std::fs::write(
        &bad_constitution_path,
        r"version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.28
  governance_schema_version: governance.v1
partitions:
  - id: domain
    path: src/domain
rules:
  - type: forbidden_dependency
    id: bad_rule
    from: domain
    to: missing_partition
    severity: hard
",
    )
    .expect("write bad constitution");
    let bad_constitution_path_arg = bad_constitution_path.to_string_lossy().to_string();
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "constitution",
            "update",
            "proj",
            "--from-file",
            &bad_constitution_path_arg,
            "--confirm",
        ],
    );
    assert_ne!(code, 0, "expected validation failure");
    assert!(
        err.contains("constitution_validation_failed") || err.contains("unknown partition"),
        "{err}"
    );

    let update_constitution_path = tmp.path().join("constitution-updated.yaml");
    std::fs::write(
        &update_constitution_path,
        r"version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.28
  governance_schema_version: governance.v1
partitions:
  - id: domain
    path: src/domain
  - id: infrastructure
    path: src/infrastructure
rules:
  - type: forbidden_dependency
    id: no_domain_to_infra
    from: domain
    to: infrastructure
    severity: hard
  - type: coverage_requirement
    id: require_domain_coverage
    target: domain
    threshold: 70
    severity: advisory
",
    )
    .expect("write update constitution");
    let update_constitution_path_arg = update_constitution_path.to_string_lossy().to_string();
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "constitution",
            "update",
            "proj",
            "--from-file",
            &update_constitution_path_arg,
            "--confirm",
            "--actor",
            "reviewer",
            "--intent",
            "add advisory coverage rule",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, project_out, err) =
        run_hivemind(tmp.path(), &["-f", "json", "project", "inspect", "proj"]);
    assert_eq!(code, 0, "{err}");
    let project_json: serde_json::Value = serde_json::from_str(&project_out).expect("project json");
    let project_data = project_json.get("data").expect("project data");
    assert!(project_data
        .get("constitution_digest")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|digest| !digest.is_empty()));
    assert_eq!(
        project_data
            .get("constitution_schema_version")
            .and_then(serde_json::Value::as_str),
        Some("constitution.v1")
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
            "600",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("constitution_initialized"),
        "{events_out}"
    );
    assert!(
        events_out.contains("constitution_validated"),
        "{events_out}"
    );
    assert!(events_out.contains("constitution_updated"), "{events_out}");
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_constitution_check_reports_blocking_violations() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);
    std::fs::create_dir_all(repo_dir.join("src/domain")).expect("create domain dir");
    std::fs::write(
        repo_dir.join("src/lib.rs"),
        "pub mod domain;\npub mod infrastructure;\n",
    )
    .expect("write lib.rs");
    std::fs::write(
        repo_dir.join("src/infrastructure.rs"),
        "pub fn db() -> &'static str { \"ok\" }\n",
    )
    .expect("write infrastructure.rs");
    std::fs::write(
        repo_dir.join("src/domain/mod.rs"),
        "pub mod extra;\nuse crate::infrastructure::db;\npub fn run() -> &'static str { db() }\n",
    )
    .expect("write domain/mod.rs");
    std::fs::write(repo_dir.join("src/domain/extra.rs"), "// no symbols\n")
        .expect("write domain/extra.rs");
    let out = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo_dir)
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
            "seed domain infrastructure graph",
        ])
        .current_dir(&repo_dir)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");
    let repo_path = repo_dir.to_string_lossy().to_string();
    let (code, _out, err) =
        run_hivemind(tmp.path(), &["project", "attach-repo", "proj", &repo_path]);
    assert_eq!(code, 0, "{err}");

    let constitution_path = tmp.path().join("constitution-check.yaml");
    std::fs::write(
        &constitution_path,
        r"version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.28
  governance_schema_version: governance.v1
partitions:
  - id: domain
    path: src/domain
  - id: infrastructure
    path: src/infrastructure.rs
rules:
  - type: forbidden_dependency
    id: no_domain_to_infra_hard
    from: domain
    to: infrastructure
    severity: hard
  - type: forbidden_dependency
    id: no_domain_to_infra_info
    from: domain
    to: infrastructure
    severity: informational
  - type: coverage_requirement
    id: domain_symbol_coverage
    target: domain
    threshold: 100
    severity: advisory
",
    )
    .expect("write constitution");
    let constitution_path_arg = constitution_path.to_string_lossy().to_string();
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "constitution",
            "init",
            "proj",
            "--from-file",
            &constitution_path_arg,
            "--confirm",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, check_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "constitution", "check", "--project", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let check_json: serde_json::Value = serde_json::from_str(&check_out).expect("check json");
    let check_data = check_json.get("data").expect("check data");
    assert_eq!(
        check_data.get("gate").and_then(serde_json::Value::as_str),
        Some("manual_check")
    );
    assert_eq!(
        check_data
            .get("blocked")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        check_data
            .get("hard_violations")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        check_data
            .get("advisory_violations")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        check_data
            .get("informational_violations")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    let violations = check_data
        .get("violations")
        .and_then(serde_json::Value::as_array)
        .expect("violations array");
    assert_eq!(violations.len(), 3);
    assert!(violations.iter().any(|item| {
        item.get("rule_id")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id == "no_domain_to_infra_hard")
    }));

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
            "500",
        ],
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        events_out.contains("constitution_violation_detected"),
        "{events_out}"
    );

    let (code, filtered_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "events",
            "list",
            "--project",
            "proj",
            "--rule-id",
            "no_domain_to_infra_hard",
            "--limit",
            "200",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let filtered_json: serde_json::Value =
        serde_json::from_str(&filtered_out).expect("filtered events json");
    let filtered_events = filtered_json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("filtered events data");
    assert!(!filtered_events.is_empty(), "{filtered_out}");
    assert!(filtered_events.iter().all(|event| {
        event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("constitution_violation_detected")
    }));
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_project_governance_diagnose_reports_invalid_refs_and_stale_snapshot() {
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
            "global",
            "system-prompt",
            "create",
            "sp-diagnose",
            "--content",
            "diagnose prompt",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "skill",
            "create",
            "skill-diagnose",
            "--name",
            "Skill Diagnose",
            "--content",
            "diagnose skill",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "template",
            "create",
            "tpl-diagnose",
            "--system-prompt-id",
            "sp-diagnose",
            "--skill-id",
            "skill-diagnose",
            "--document-id",
            "doc-missing",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, diagnose_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "diagnose", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let diagnose_json: serde_json::Value =
        serde_json::from_str(&diagnose_out).expect("diagnose json");
    let diagnose_data = diagnose_json.get("data").expect("diagnose data");
    assert_eq!(
        diagnose_data
            .get("healthy")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    let issue_codes: Vec<String> = diagnose_data
        .get("issues")
        .and_then(serde_json::Value::as_array)
        .expect("issues array")
        .iter()
        .filter_map(|issue| issue.get("code").and_then(serde_json::Value::as_str))
        .map(std::string::ToString::to_string)
        .collect();
    assert!(
        issue_codes
            .iter()
            .any(|code| code == "template_document_missing"),
        "{diagnose_out}"
    );

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "doc-missing",
            "--title",
            "Doc Missing",
            "--owner",
            "owner",
            "--content",
            "content",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["graph", "snapshot", "refresh", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, healthy_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "diagnose", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let healthy_json: serde_json::Value =
        serde_json::from_str(&healthy_out).expect("healthy diagnose json");
    let healthy_data = healthy_json.get("data").expect("healthy diagnose data");
    assert_eq!(
        healthy_data
            .get("healthy")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "{healthy_out}"
    );
    assert_eq!(
        healthy_data
            .get("issue_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "{healthy_out}"
    );

    std::fs::write(repo_dir.join("README.md"), "stale snapshot trigger\n").expect("write readme");
    let out = Command::new("git")
        .args(["add", "README.md"])
        .current_dir(&repo_dir)
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
            "stale snapshot trigger",
        ])
        .current_dir(&repo_dir)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (code, stale_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "diagnose", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let stale_json: serde_json::Value = serde_json::from_str(&stale_out).expect("stale diagnose");
    let stale_issues = stale_json
        .get("data")
        .and_then(|d| d.get("issues"))
        .and_then(serde_json::Value::as_array)
        .expect("stale issues");
    assert!(stale_issues.iter().any(|issue| {
        issue.get("code").and_then(serde_json::Value::as_str) == Some("graph_snapshot_stale")
    }));
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_governance_artifact_ops_stable_under_concurrent_flow_activity() {
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
            "Doc One",
            "--owner",
            "owner",
            "--content",
            "revision-0",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "system-prompt",
            "create",
            "sp-concurrent",
            "--content",
            "concurrent prompt",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "skill",
            "create",
            "skill-concurrent",
            "--name",
            "Skill Concurrent",
            "--content",
            "concurrent skill",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "global",
            "template",
            "create",
            "tpl-concurrent",
            "--system-prompt-id",
            "sp-concurrent",
            "--skill-id",
            "skill-concurrent",
            "--document-id",
            "doc1",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = set_project_runtime_script(
        tmp.path(),
        "proj",
        "echo runtime_ok; \"$HIVEMIND_BIN\" checkpoint complete --id checkpoint-1",
        "echo runtime_ok & \"%HIVEMIND_BIN%\" checkpoint complete --id checkpoint-1",
        2000,
    );
    assert_eq!(code, 0, "{err}");

    let mut flow_ids = Vec::new();
    for title in ["t1", "t2"] {
        let (code, out, err) = run_hivemind(tmp.path(), &["task", "create", "proj", title]);
        assert_eq!(code, 0, "{err}");
        let task_id = out
            .lines()
            .find_map(|l| l.strip_prefix("ID:").map(|s| s.trim().to_string()))
            .expect("task id");
        let (code, gout, err) = run_hivemind(
            tmp.path(),
            &[
                "graph",
                "create",
                "proj",
                &format!("g-{title}"),
                "--from-tasks",
                &task_id,
            ],
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
        flow_ids.push(flow_id);
    }

    let home = tmp.path().to_path_buf();
    let flow1 = flow_ids[0].clone();
    let home1 = home.clone();
    let t1 = std::thread::spawn(move || run_hivemind(&home1, &["flow", "tick", &flow1]));

    let flow2 = flow_ids[1].clone();
    let home2 = home.clone();
    let t2 = std::thread::spawn(move || run_hivemind(&home2, &["flow", "tick", &flow2]));

    for idx in 1..=6 {
        let (code, _out, err) = run_hivemind(
            &home,
            &[
                "global",
                "template",
                "instantiate",
                "proj",
                "tpl-concurrent",
            ],
        );
        assert_eq!(code, 0, "{err}");
        let revision = format!("revision-{idx}");
        let (code, _out, err) = run_hivemind(
            &home,
            &[
                "project",
                "governance",
                "document",
                "update",
                "proj",
                "doc1",
                "--content",
                &revision,
            ],
        );
        assert_eq!(code, 0, "{err}");
    }

    let (code1, _out1, err1) = t1.join().expect("tick thread 1");
    assert_eq!(code1, 0, "{err1}");
    let (code2, _out2, err2) = t2.join().expect("tick thread 2");
    assert_eq!(code2, 0, "{err2}");

    let (code, out, err) = run_hivemind(
        &home,
        &[
            "-f",
            "json",
            "events",
            "list",
            "--project",
            "proj",
            "--template-id",
            "tpl-concurrent",
            "--limit",
            "300",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).expect("events json");
    let events = json
        .get("data")
        .and_then(serde_json::Value::as_array)
        .expect("events array");
    assert!(events.iter().any(|event| {
        event
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(serde_json::Value::as_str)
            == Some("template_instantiated")
    }));
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_governance_snapshot_restore_and_repair_flow() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, doc_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "arch-doc",
            "--title",
            "Architecture",
            "--owner",
            "ops",
            "--content",
            "baseline-v1",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let doc_json: serde_json::Value = serde_json::from_str(&doc_out).expect("document json");
    let document_path = doc_json
        .get("data")
        .and_then(|d| d.get("path"))
        .and_then(serde_json::Value::as_str)
        .expect("document path")
        .to_string();
    assert!(std::path::Path::new(&document_path).is_file());

    let (code, snap_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "snapshot",
            "create",
            "proj",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let snap_json: serde_json::Value = serde_json::from_str(&snap_out).expect("snapshot json");
    let snapshot_id = snap_json
        .get("data")
        .and_then(|d| d.get("snapshot"))
        .and_then(|d| d.get("snapshot_id"))
        .and_then(serde_json::Value::as_str)
        .expect("snapshot id")
        .to_string();

    std::fs::remove_file(&document_path).expect("remove governance document");

    let (code, detect_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "repair",
            "detect",
            "proj",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let detect_json: serde_json::Value = serde_json::from_str(&detect_out).expect("detect json");
    assert!(
        detect_json
            .get("data")
            .and_then(|d| d.get("issue_count"))
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|count| count >= 1),
        "expected at least one drift issue: {detect_out}"
    );

    let (code, preview_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "repair",
            "preview",
            "proj",
            "--snapshot-id",
            &snapshot_id,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let preview_json: serde_json::Value = serde_json::from_str(&preview_out).expect("preview json");
    let has_restore_operation = preview_json
        .get("data")
        .and_then(|d| d.get("operations"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|ops| {
            ops.iter().any(|item| {
                item.get("action").and_then(serde_json::Value::as_str)
                    == Some("restore_from_snapshot")
            })
        });
    assert!(
        has_restore_operation,
        "expected restore operation in preview: {preview_out}"
    );

    let (code, apply_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "repair",
            "apply",
            "proj",
            "--snapshot-id",
            &snapshot_id,
            "--confirm",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let apply_json: serde_json::Value = serde_json::from_str(&apply_out).expect("apply json");
    assert_eq!(
        apply_json
            .get("data")
            .and_then(|d| d.get("remaining_issue_count"))
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "expected repair to clear drift issues: {apply_out}"
    );

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
            "arch-doc",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value = serde_json::from_str(&inspect_out).expect("inspect json");
    assert_eq!(
        inspect_json
            .get("data")
            .and_then(|d| d.get("latest_content"))
            .and_then(serde_json::Value::as_str),
        Some("baseline-v1")
    );
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_governance_replay_and_snapshot_restore_verification() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "runbook",
            "--title",
            "Runbook",
            "--owner",
            "ops",
            "--content",
            "v1-content",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, replay_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "replay",
            "proj",
            "--verify",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let replay_json: serde_json::Value = serde_json::from_str(&replay_out).expect("replay json");
    assert_eq!(
        replay_json
            .get("data")
            .and_then(|d| d.get("idempotent"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        replay_json
            .get("data")
            .and_then(|d| d.get("current_matches_replay"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let (code, create_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "snapshot",
            "create",
            "proj",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let create_json: serde_json::Value =
        serde_json::from_str(&create_out).expect("snapshot create");
    let snapshot_id = create_json
        .get("data")
        .and_then(|d| d.get("snapshot"))
        .and_then(|d| d.get("snapshot_id"))
        .and_then(serde_json::Value::as_str)
        .expect("snapshot id")
        .to_string();

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
            "runbook",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value = serde_json::from_str(&inspect_out).expect("inspect");
    let document_path = inspect_json
        .get("data")
        .and_then(|d| d.get("summary"))
        .and_then(|d| d.get("path"))
        .and_then(serde_json::Value::as_str)
        .expect("document path")
        .to_string();

    std::fs::write(&document_path, "{broken json").expect("corrupt document file");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "project",
            "governance",
            "snapshot",
            "restore",
            "proj",
            &snapshot_id,
            "--confirm",
        ],
    );
    assert_eq!(code, 0, "{err}");

    let (code, post_out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "document",
            "inspect",
            "proj",
            "runbook",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let post_json: serde_json::Value = serde_json::from_str(&post_out).expect("post inspect");
    assert_eq!(
        post_json
            .get("data")
            .and_then(|d| d.get("latest_content"))
            .and_then(serde_json::Value::as_str),
        Some("v1-content")
    );
}

#[test]
// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
fn cli_governance_replay_verify_and_diagnose_detect_missing_artifact_files() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "create", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(tmp.path(), &["project", "governance", "init", "proj"]);
    assert_eq!(code, 0, "{err}");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "document",
            "create",
            "proj",
            "doc-missing",
            "--title",
            "Doc Missing",
            "--owner",
            "ops",
            "--content",
            "hello",
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
            "doc-missing",
        ],
    );
    assert_eq!(code, 0, "{err}");
    let inspect_json: serde_json::Value = serde_json::from_str(&inspect_out).expect("inspect");
    let document_path = inspect_json
        .get("data")
        .and_then(|d| d.get("summary"))
        .and_then(|d| d.get("path"))
        .and_then(serde_json::Value::as_str)
        .expect("document path")
        .to_string();
    std::fs::remove_file(&document_path).expect("remove governance document");

    let (code, _out, err) = run_hivemind(
        tmp.path(),
        &[
            "-f",
            "json",
            "project",
            "governance",
            "replay",
            "proj",
            "--verify",
        ],
    );
    assert_ne!(
        code, 0,
        "replay --verify should fail when projection files are missing"
    );
    let replay_err_json: serde_json::Value =
        serde_json::from_str(err.trim()).expect("replay verify error json");
    assert_eq!(
        replay_err_json
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(serde_json::Value::as_str),
        Some("governance_replay_verification_failed"),
        "{err}"
    );

    let (code, diagnose_out, err) = run_hivemind(
        tmp.path(),
        &["-f", "json", "project", "governance", "diagnose", "proj"],
    );
    assert_eq!(code, 0, "{err}");
    let diagnose_json: serde_json::Value =
        serde_json::from_str(&diagnose_out).expect("diagnose json");
    let diagnose_data = diagnose_json.get("data").expect("diagnose data");
    assert_eq!(
        diagnose_data
            .get("healthy")
            .and_then(serde_json::Value::as_bool),
        Some(false),
        "{diagnose_out}"
    );
    let issue_codes: Vec<String> = diagnose_data
        .get("issues")
        .and_then(serde_json::Value::as_array)
        .expect("issues array")
        .iter()
        .filter_map(|issue| issue.get("code").and_then(serde_json::Value::as_str))
        .map(std::string::ToString::to_string)
        .collect();
    assert!(
        issue_codes
            .iter()
            .any(|code| code == "governance_artifact_missing"),
        "{diagnose_out}"
    );
}
