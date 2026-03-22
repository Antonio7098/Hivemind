use super::*;
use tempfile::tempdir;

fn query_scalar_i64(db_path: &Path, sql: &str) -> i64 {
    let output = Command::new("sqlite3")
        .arg("-noheader")
        .arg("-batch")
        .arg(db_path)
        .arg(sql)
        .output()
        .expect("sqlite3 query should run");
    assert!(
        output.status.success(),
        "sqlite3 query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    raw.parse::<i64>().expect("integer sqlite output")
}

#[test]
fn runtime_state_store_applies_wal_and_migrations() {
    let dir = tempdir().expect("temp dir");
    let mut env = HashMap::new();
    env.insert(
        STATE_DB_PATH_ENV.to_string(),
        dir.path()
            .join("native-state.sqlite")
            .to_string_lossy()
            .to_string(),
    );
    let config = RuntimeHardeningConfig::from_env(&env);
    let store = NativeRuntimeStateStore::open(&config).expect("state store should initialize");

    let mode = Command::new("sqlite3")
        .arg("-noheader")
        .arg("-batch")
        .arg(&store.db_path)
        .arg("PRAGMA journal_mode;")
        .output()
        .expect("sqlite3 pragma query");
    assert!(mode.status.success());
    let journal_mode = String::from_utf8_lossy(&mode.stdout).trim().to_lowercase();
    assert_eq!(journal_mode, "wal");

    let migrations = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM schema_migrations WHERE version IN (1,2);",
    );
    assert_eq!(migrations, 2);
}

#[test]
fn runtime_log_ingestor_batches_and_survives_restart() {
    let dir = tempdir().expect("temp dir");
    let db_path = dir.path().join("state.sqlite");
    let mut env = HashMap::new();
    env.insert(
        STATE_DB_PATH_ENV.to_string(),
        db_path.to_string_lossy().to_string(),
    );
    env.insert(
        SECRETS_STORE_PATH_ENV.to_string(),
        dir.path()
            .join("secrets.enc.json")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        SECRETS_KEYRING_PATH_ENV.to_string(),
        dir.path()
            .join("secrets.keyring")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(LOG_BATCH_SIZE_ENV.to_string(), "2".to_string());

    let support = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap should pass");
    support
        .ingest_log("native_bootstrap", "info", "line1", None)
        .expect("log1");
    support
        .ingest_log("native_bootstrap", "info", "line2", None)
        .expect("log2");
    support.flush_logs().expect("flush");
    support.shutdown().expect("shutdown");

    let support_restart = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap restart");
    support_restart.shutdown().expect("shutdown restart");

    let count = query_scalar_i64(&db_path, "SELECT COUNT(1) FROM runtime_logs;");
    assert!(count >= 2, "expected persisted runtime logs across restart");
}

#[test]
fn runtime_state_store_persists_graphcode_registry_records() {
    let dir = tempdir().expect("temp dir");
    let config = RuntimeHardeningConfig::for_state_dir(dir.path());
    let store = NativeRuntimeStateStore::open(&config).expect("state store should initialize");

    store
        .upsert_graphcode_artifact(&GraphCodeArtifactUpsert {
            registry_key: "project-1:codegraph".to_string(),
            project_id: "project-1".to_string(),
            substrate_kind: "codegraph".to_string(),
            storage_backend: "sqlite".to_string(),
            storage_reference: "graphcode_artifacts/project-1:codegraph".to_string(),
            derivative_snapshot_path: Some("/tmp/graph_snapshot.json".to_string()),
            constitution_path: Some("/tmp/constitution.yaml".to_string()),
            canonical_fingerprint: "fp-123".to_string(),
            profile_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
            ucp_engine_version: ucp_api::CODEGRAPH_EXTRACTOR_VERSION.to_string(),
            extractor_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
            runtime_version: env!("CARGO_PKG_VERSION").to_string(),
            freshness_state: "current".to_string(),
            repo_manifest_json: "[{\"repo_name\":\"repo\"}]".to_string(),
            active_session_ref: Some("project-1:codegraph:session:default".to_string()),
            snapshot_json: "{\"canonical_fingerprint\":\"fp-123\"}".to_string(),
        })
        .expect("artifact upsert");
    store
        .upsert_graphcode_session(&GraphCodeSessionUpsert {
            session_ref: "project-1:codegraph:session:default".to_string(),
            registry_key: "project-1:codegraph".to_string(),
            substrate_kind: "codegraph".to_string(),
            current_focus_json: "[\"node-1\"]".to_string(),
            pinned_nodes_json: "[]".to_string(),
            recent_traversals_json: "[{\"query_kind\":\"filter\"}]".to_string(),
            working_set_refs_json: "[\"node-1\"]".to_string(),
            hydrated_excerpts_json: "[]".to_string(),
            path_artifacts_json: "[]".to_string(),
            snapshot_fingerprint: "fp-123".to_string(),
            freshness_state: "current".to_string(),
        })
        .expect("session upsert");

    let record = store
        .graphcode_artifact_by_project("project-1", "codegraph")
        .expect("read graphcode record")
        .expect("record should exist");
    assert_eq!(record.canonical_fingerprint, "fp-123");
    assert_eq!(record.storage_backend, "sqlite");

    let by_registry = store
        .graphcode_artifact_by_registry_key("project-1:codegraph")
        .expect("read by registry key")
        .expect("record should exist by registry key");
    assert_eq!(by_registry.project_id, "project-1");

    let session = store
        .graphcode_session_by_ref("project-1:codegraph:session:default")
        .expect("read session")
        .expect("session should exist");
    assert_eq!(session.snapshot_fingerprint, "fp-123");

    store
        .mark_graphcode_artifact_freshness_by_registry_key("project-1:codegraph", "stale")
        .expect("mark artifact stale by registry key");
    store
        .mark_graphcode_session_freshness_by_registry_key("project-1:codegraph", "stale")
        .expect("mark session stale by registry key");

    let artifact_freshness = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_artifacts WHERE registry_key = 'project-1:codegraph' AND freshness_state = 'stale';",
    );
    assert_eq!(artifact_freshness, 1);

    let session_freshness = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_sessions WHERE registry_key = 'project-1:codegraph' AND freshness_state = 'stale';",
    );
    assert_eq!(session_freshness, 1);

    let session_count = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_sessions WHERE registry_key = 'project-1:codegraph';",
    );
    assert_eq!(session_count, 1);
}

#[test]
fn runtime_state_store_marks_graphcode_records_by_registry_prefix() {
    let dir = tempdir().expect("temp dir");
    let config = RuntimeHardeningConfig::for_state_dir(dir.path());
    let store = NativeRuntimeStateStore::open(&config).expect("state store should initialize");

    for registry_key in [
        "project-1:codegraph",
        "project-1:codegraph:attempt:attempt-1",
        "project-2:codegraph:attempt:attempt-2",
    ] {
        let session_ref = format!("{registry_key}:session:default");
        store
            .upsert_graphcode_artifact(&GraphCodeArtifactUpsert {
                registry_key: registry_key.to_string(),
                project_id: registry_key
                    .split(':')
                    .next()
                    .unwrap_or("project")
                    .to_string(),
                substrate_kind: "codegraph".to_string(),
                storage_backend: "sqlite".to_string(),
                storage_reference: format!("graphcode_artifacts/{registry_key}"),
                derivative_snapshot_path: None,
                constitution_path: None,
                canonical_fingerprint: format!("fp-{registry_key}"),
                profile_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
                ucp_engine_version: ucp_api::CODEGRAPH_EXTRACTOR_VERSION.to_string(),
                extractor_version: ucp_api::CODEGRAPH_PROFILE_MARKER.to_string(),
                runtime_version: env!("CARGO_PKG_VERSION").to_string(),
                freshness_state: "current".to_string(),
                repo_manifest_json: "[]".to_string(),
                active_session_ref: Some(session_ref.clone()),
                snapshot_json: "{}".to_string(),
            })
            .expect("artifact upsert");
        store
            .upsert_graphcode_session(&GraphCodeSessionUpsert {
                session_ref,
                registry_key: registry_key.to_string(),
                substrate_kind: "codegraph".to_string(),
                current_focus_json: "[]".to_string(),
                pinned_nodes_json: "[]".to_string(),
                recent_traversals_json: "[]".to_string(),
                working_set_refs_json: "[]".to_string(),
                hydrated_excerpts_json: "[]".to_string(),
                path_artifacts_json: "[]".to_string(),
                snapshot_fingerprint: format!("fp-{registry_key}"),
                freshness_state: "current".to_string(),
            })
            .expect("session upsert");
    }

    store
        .mark_graphcode_artifact_freshness_by_registry_prefix(
            "project-1:codegraph:attempt:",
            "stale",
        )
        .expect("mark artifacts by prefix");
    store
        .mark_graphcode_session_freshness_by_registry_prefix(
            "project-1:codegraph:attempt:",
            "stale",
        )
        .expect("mark sessions by prefix");

    let stale_artifacts = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_artifacts WHERE registry_key LIKE 'project-1:codegraph:attempt:%' AND freshness_state = 'stale';",
    );
    assert_eq!(stale_artifacts, 1);

    let stale_sessions = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_sessions WHERE registry_key LIKE 'project-1:codegraph:attempt:%' AND freshness_state = 'stale';",
    );
    assert_eq!(stale_sessions, 1);

    let untouched_authoritative = query_scalar_i64(
        &store.db_path,
        "SELECT COUNT(1) FROM graphcode_artifacts WHERE registry_key = 'project-1:codegraph' AND freshness_state = 'current';",
    );
    assert_eq!(untouched_authoritative, 1);
}
