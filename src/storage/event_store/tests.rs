use super::*;
use crate::core::error::HivemindError;
use crate::core::events::{CorrelationIds, EventPayload};

#[test]
fn in_memory_store_append_and_read() {
    let store = InMemoryEventStore::new();
    let project_id = Uuid::new_v4();

    let event = Event::new(
        EventPayload::ProjectCreated {
            id: project_id,
            name: "test".to_string(),
            description: None,
        },
        CorrelationIds::for_project(project_id),
    );

    let id = store.append(event).unwrap();
    let events = store.read_all().unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id(), id);
}

#[test]
fn in_memory_store_filter_by_project() {
    let store = InMemoryEventStore::new();
    let project1 = Uuid::new_v4();
    let project2 = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project1,
                name: "p1".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project1),
        ))
        .unwrap();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project2,
                name: "p2".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project2),
        ))
        .unwrap();

    let filter = EventFilter::for_project(project1);
    let events = store.read(&filter).unwrap();

    assert_eq!(events.len(), 1);
}

#[test]
fn in_memory_store_filter_by_graph() {
    let store = InMemoryEventStore::new();
    let project = Uuid::new_v4();
    let graph1 = Uuid::new_v4();
    let graph2 = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::TaskGraphCreated {
                graph_id: graph1,
                project_id: project,
                name: "g1".to_string(),
                description: None,
            },
            CorrelationIds::for_graph(project, graph1),
        ))
        .unwrap();

    store
        .append(Event::new(
            EventPayload::TaskGraphCreated {
                graph_id: graph2,
                project_id: project,
                name: "g2".to_string(),
                description: None,
            },
            CorrelationIds::for_graph(project, graph2),
        ))
        .unwrap();

    let filter = EventFilter::for_graph(graph1);
    let events = store.read(&filter).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].metadata.correlation.graph_id, Some(graph1));
}

#[test]
fn in_memory_store_filter_by_time_range() {
    let store = InMemoryEventStore::new();
    let project = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project,
                name: "p1".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();
    store
        .append(Event::new(
            EventPayload::ProjectUpdated {
                id: project,
                name: Some("p2".to_string()),
                description: None,
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();

    let all = store.read_all().unwrap();
    assert_eq!(all.len(), 2);
    let first_ts = all[0].metadata.timestamp;
    let second_ts = all[1].metadata.timestamp;

    let mut filter = EventFilter::all();
    filter.since = Some(second_ts);
    let since_events = store.read(&filter).unwrap();
    assert_eq!(since_events.len(), 1);
    assert_eq!(since_events[0].metadata.timestamp, second_ts);

    let mut filter = EventFilter::all();
    filter.until = Some(first_ts);
    let until_events = store.read(&filter).unwrap();
    assert_eq!(until_events.len(), 1);
    assert_eq!(until_events[0].metadata.timestamp, first_ts);
}

#[test]
fn in_memory_store_filters_governance_artifact_template_and_rule_ids() {
    let store = InMemoryEventStore::new();
    let project = Uuid::new_v4();
    let flow = Uuid::new_v4();
    let task = Uuid::new_v4();
    let attempt = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::GovernanceArtifactUpserted {
                project_id: Some(project),
                scope: "project".to_string(),
                artifact_kind: "document".to_string(),
                artifact_key: "doc-1".to_string(),
                path: "/tmp/doc-1.json".to_string(),
                revision: 1,
                schema_version: "governance.v1".to_string(),
                projection_version: 1,
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();

    store
        .append(Event::new(
            EventPayload::TemplateInstantiated {
                project_id: project,
                template_id: "tpl-1".to_string(),
                system_prompt_id: "prompt-1".to_string(),
                skill_ids: vec!["skill-1".to_string()],
                document_ids: vec!["doc-1".to_string()],
                schema_version: "governance.v1".to_string(),
                projection_version: 1,
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();

    store
        .append(Event::new(
            EventPayload::ConstitutionViolationDetected {
                project_id: project,
                flow_id: Some(flow),
                task_id: Some(task),
                attempt_id: Some(attempt),
                gate: "manual_check".to_string(),
                rule_id: "rule-hard".to_string(),
                rule_type: "forbidden_dependency".to_string(),
                severity: "hard".to_string(),
                message: "blocked".to_string(),
                evidence: vec!["a -> b".to_string()],
                remediation_hint: Some("fix dependency".to_string()),
                blocked: true,
            },
            CorrelationIds::for_flow_task(project, flow, task),
        ))
        .unwrap();

    let mut artifact_filter = EventFilter::all();
    artifact_filter.artifact_id = Some("doc-1".to_string());
    let artifact_events = store.read(&artifact_filter).unwrap();
    assert_eq!(artifact_events.len(), 2);

    let mut template_filter = EventFilter::all();
    template_filter.template_id = Some("tpl-1".to_string());
    let template_events = store.read(&template_filter).unwrap();
    assert_eq!(template_events.len(), 1);
    assert!(matches!(
        template_events[0].payload,
        EventPayload::TemplateInstantiated { .. }
    ));

    let mut rule_filter = EventFilter::all();
    rule_filter.rule_id = Some("rule-hard".to_string());
    let rule_events = store.read(&rule_filter).unwrap();
    assert_eq!(rule_events.len(), 1);
    assert!(matches!(
        rule_events[0].payload,
        EventPayload::ConstitutionViolationDetected { .. }
    ));
}

#[test]
fn in_memory_store_filters_error_events_by_error_type() {
    let store = InMemoryEventStore::new();
    let project = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ErrorOccurred {
                error: HivemindError::user(
                    "task_not_in_flow",
                    "Task is not part of any flow",
                    "registry:start_task_execution",
                ),
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project,
                name: "proj".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project),
        ))
        .unwrap();

    let mut filter = EventFilter::all();
    filter.error_type = Some("user".to_string());
    let events = store.read(&filter).unwrap();

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].payload,
        EventPayload::ErrorOccurred { .. }
    ));
}

#[test]
fn file_store_persist_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");

    let project_id = Uuid::new_v4();
    let event = Event::new(
        EventPayload::ProjectCreated {
            id: project_id,
            name: "persist-test".to_string(),
            description: None,
        },
        CorrelationIds::for_project(project_id),
    );

    {
        let store = FileEventStore::open(path.clone()).unwrap();
        store.append(event.clone()).unwrap();
    }

    {
        let store = FileEventStore::open(path).unwrap();
        let events = store.read_all().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].payload, event.payload);
    }
}

#[test]
fn file_store_ignores_unknown_event_payload_types() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");

    let project_id = Uuid::new_v4();
    let event = Event::new(
        EventPayload::ProjectCreated {
            id: project_id,
            name: "persist-test".to_string(),
            description: None,
        },
        CorrelationIds::for_project(project_id),
    );

    let mut value = serde_json::to_value(&event).unwrap();
    value["payload"]["type"] = serde_json::json!("future_event_type");
    value["payload"]["some_new_field"] = serde_json::json!("some_value");
    let unknown_line = serde_json::to_string(&value).unwrap();

    std::fs::write(&path, format!("{unknown_line}\n")).unwrap();

    let store = FileEventStore::open(path).unwrap();
    let events = store.read_all().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].payload, EventPayload::Unknown);
}

#[test]
fn indexed_store_writes_project_and_flow_mirrors() {
    let dir = tempfile::tempdir().unwrap();
    let store = IndexedEventStore::open(dir.path()).unwrap();
    let project_id = Uuid::new_v4();
    let flow_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ErrorOccurred {
                error: HivemindError::user(
                    "indexed_store_test",
                    "indexed mirror test",
                    "storage:event_store:test",
                ),
            },
            CorrelationIds::for_flow_task(project_id, flow_id, task_id),
        ))
        .unwrap();

    let global_events = store.read_all().unwrap();
    assert_eq!(global_events.len(), 1);

    let index_json = std::fs::read_to_string(dir.path().join("index.json")).unwrap();
    assert!(index_json.contains(&project_id.to_string()), "{index_json}");

    let project_log = std::fs::read_to_string(
        dir.path()
            .join("projects")
            .join(project_id.to_string())
            .join("events.jsonl"),
    )
    .unwrap();
    assert!(project_log.contains("indexed_store_test"), "{project_log}");

    let flow_log = std::fs::read_to_string(
        dir.path()
            .join("flows")
            .join(flow_id.to_string())
            .join("events.jsonl"),
    )
    .unwrap();
    assert!(flow_log.contains("indexed_store_test"), "{flow_log}");
}

#[test]
fn sqlite_store_append_read_and_reload() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteEventStore::open(dir.path()).unwrap();
    let project_id = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "db-project".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        ))
        .unwrap();

    let events = store.read_all().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].metadata.sequence, Some(0));

    let store_reloaded = SqliteEventStore::open(dir.path()).unwrap();
    let events_reloaded = store_reloaded.read_all().unwrap();
    assert_eq!(events_reloaded.len(), 1);
    assert!(matches!(
        events_reloaded[0].payload,
        EventPayload::ProjectCreated { .. }
    ));
}

#[test]
fn sqlite_store_mirrors_legacy_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteEventStore::open(dir.path()).unwrap();
    let project_id = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "mirror-project".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        ))
        .unwrap();

    let mirror_path = dir.path().join("events.jsonl");
    let mirror = std::fs::read_to_string(mirror_path).unwrap();
    assert!(mirror.contains("\"type\":\"project_created\""));
}

#[test]
fn sqlite_store_enforces_append_only_triggers() {
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteEventStore::open(dir.path()).unwrap();
    let project_id = Uuid::new_v4();

    store
        .append(Event::new(
            EventPayload::ProjectCreated {
                id: project_id,
                name: "append-only-project".to_string(),
                description: None,
            },
            CorrelationIds::for_project(project_id),
        ))
        .unwrap();

    let db_path = dir.path().join("db.sqlite");
    let update_err = SqliteEventStore::run_sql_batch_on_path(
        &db_path,
        "UPDATE events SET event_json = '{\"mutated\":true}' WHERE sequence = 0;",
    )
    .unwrap_err();
    let update_msg = update_err.to_string();
    assert!(
        update_msg.contains("events_append_only_violation"),
        "{update_msg}"
    );

    let delete_err =
        SqliteEventStore::run_sql_batch_on_path(&db_path, "DELETE FROM events WHERE sequence = 0;")
            .unwrap_err();
    let delete_msg = delete_err.to_string();
    assert!(
        delete_msg.contains("events_append_only_violation"),
        "{delete_msg}"
    );

    let events = store.read_all().unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0].payload,
        EventPayload::ProjectCreated { .. }
    ));
}

