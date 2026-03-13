use super::*;

impl NativeRuntimeStateStore {
    pub(crate) fn upsert_graphcode_artifact(
        &self,
        artifact: &GraphCodeArtifactUpsert,
    ) -> Result<()> {
        let updated_at_ms = now_ms();
        self.exec(&format!(
            "INSERT INTO graphcode_artifacts (registry_key, project_id, substrate_kind, storage_backend, storage_reference, derivative_snapshot_path, constitution_path, canonical_fingerprint, profile_version, ucp_engine_version, extractor_version, runtime_version, freshness_state, repo_manifest_json, active_session_ref, snapshot_json, updated_at_ms) VALUES ('{registry_key}', '{project_id}', '{substrate_kind}', '{storage_backend}', '{storage_reference}', {derivative_snapshot_path}, {constitution_path}, '{canonical_fingerprint}', '{profile_version}', '{ucp_engine_version}', '{extractor_version}', '{runtime_version}', '{freshness_state}', '{repo_manifest_json}', {active_session_ref}, '{snapshot_json}', {updated_at_ms}) ON CONFLICT(registry_key) DO UPDATE SET project_id=excluded.project_id, substrate_kind=excluded.substrate_kind, storage_backend=excluded.storage_backend, storage_reference=excluded.storage_reference, derivative_snapshot_path=excluded.derivative_snapshot_path, constitution_path=excluded.constitution_path, canonical_fingerprint=excluded.canonical_fingerprint, profile_version=excluded.profile_version, ucp_engine_version=excluded.ucp_engine_version, extractor_version=excluded.extractor_version, runtime_version=excluded.runtime_version, freshness_state=excluded.freshness_state, repo_manifest_json=excluded.repo_manifest_json, active_session_ref=excluded.active_session_ref, snapshot_json=excluded.snapshot_json, updated_at_ms=excluded.updated_at_ms;",
            registry_key = sql_escape(&artifact.registry_key),
            project_id = sql_escape(&artifact.project_id),
            substrate_kind = sql_escape(&artifact.substrate_kind),
            storage_backend = sql_escape(&artifact.storage_backend),
            storage_reference = sql_escape(&artifact.storage_reference),
            derivative_snapshot_path = sql_nullable(&artifact.derivative_snapshot_path),
            constitution_path = sql_nullable(&artifact.constitution_path),
            canonical_fingerprint = sql_escape(&artifact.canonical_fingerprint),
            profile_version = sql_escape(&artifact.profile_version),
            ucp_engine_version = sql_escape(&artifact.ucp_engine_version),
            extractor_version = sql_escape(&artifact.extractor_version),
            runtime_version = sql_escape(&artifact.runtime_version),
            freshness_state = sql_escape(&artifact.freshness_state),
            repo_manifest_json = sql_escape(&artifact.repo_manifest_json),
            active_session_ref = sql_nullable(&artifact.active_session_ref),
            snapshot_json = sql_escape(&artifact.snapshot_json),
        ))
    }

    pub(crate) fn upsert_graphcode_session(&self, session: &GraphCodeSessionUpsert) -> Result<()> {
        let updated_at_ms = now_ms();
        self.exec(&format!(
            "INSERT INTO graphcode_sessions (session_ref, registry_key, substrate_kind, current_focus_json, pinned_nodes_json, recent_traversals_json, working_set_refs_json, hydrated_excerpts_json, path_artifacts_json, snapshot_fingerprint, freshness_state, updated_at_ms) VALUES ('{session_ref}', '{registry_key}', '{substrate_kind}', '{current_focus_json}', '{pinned_nodes_json}', '{recent_traversals_json}', '{working_set_refs_json}', '{hydrated_excerpts_json}', '{path_artifacts_json}', '{snapshot_fingerprint}', '{freshness_state}', {updated_at_ms}) ON CONFLICT(session_ref) DO UPDATE SET registry_key=excluded.registry_key, substrate_kind=excluded.substrate_kind, current_focus_json=excluded.current_focus_json, pinned_nodes_json=excluded.pinned_nodes_json, recent_traversals_json=excluded.recent_traversals_json, working_set_refs_json=excluded.working_set_refs_json, hydrated_excerpts_json=excluded.hydrated_excerpts_json, path_artifacts_json=excluded.path_artifacts_json, snapshot_fingerprint=excluded.snapshot_fingerprint, freshness_state=excluded.freshness_state, updated_at_ms=excluded.updated_at_ms;",
            session_ref = sql_escape(&session.session_ref),
            registry_key = sql_escape(&session.registry_key),
            substrate_kind = sql_escape(&session.substrate_kind),
            current_focus_json = sql_escape(&session.current_focus_json),
            pinned_nodes_json = sql_escape(&session.pinned_nodes_json),
            recent_traversals_json = sql_escape(&session.recent_traversals_json),
            working_set_refs_json = sql_escape(&session.working_set_refs_json),
            hydrated_excerpts_json = sql_escape(&session.hydrated_excerpts_json),
            path_artifacts_json = sql_escape(&session.path_artifacts_json),
            snapshot_fingerprint = sql_escape(&session.snapshot_fingerprint),
            freshness_state = sql_escape(&session.freshness_state),
        ))
    }

    pub(crate) fn graphcode_artifact_by_project(
        &self,
        project_id: &str,
        substrate_kind: &str,
    ) -> Result<Option<GraphCodeArtifactRecord>> {
        let raw = self.scalar_string(&format!(
            "SELECT json_object('registry_key', registry_key, 'project_id', project_id, 'substrate_kind', substrate_kind, 'storage_backend', storage_backend, 'storage_reference', storage_reference, 'derivative_snapshot_path', derivative_snapshot_path, 'constitution_path', constitution_path, 'canonical_fingerprint', canonical_fingerprint, 'profile_version', profile_version, 'ucp_engine_version', ucp_engine_version, 'extractor_version', extractor_version, 'runtime_version', runtime_version, 'freshness_state', freshness_state, 'repo_manifest_json', repo_manifest_json, 'active_session_ref', active_session_ref, 'snapshot_json', snapshot_json, 'updated_at_ms', updated_at_ms) FROM graphcode_artifacts WHERE project_id = '{project_id}' AND substrate_kind = '{substrate_kind}' ORDER BY updated_at_ms DESC LIMIT 1;",
            project_id = sql_escape(project_id),
            substrate_kind = sql_escape(substrate_kind),
        ))?;
        raw.map(|value| {
            serde_json::from_str::<GraphCodeArtifactRecord>(&value).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_graphcode_record_decode_failed",
                    format!("Failed to decode GraphCode artifact record: {error}"),
                )
            })
        })
        .transpose()
    }

    pub(crate) fn graphcode_artifact_by_registry_key(
        &self,
        registry_key: &str,
    ) -> Result<Option<GraphCodeArtifactRecord>> {
        let raw = self.scalar_string(&format!(
            "SELECT json_object('registry_key', registry_key, 'project_id', project_id, 'substrate_kind', substrate_kind, 'storage_backend', storage_backend, 'storage_reference', storage_reference, 'derivative_snapshot_path', derivative_snapshot_path, 'constitution_path', constitution_path, 'canonical_fingerprint', canonical_fingerprint, 'profile_version', profile_version, 'ucp_engine_version', ucp_engine_version, 'extractor_version', extractor_version, 'runtime_version', runtime_version, 'freshness_state', freshness_state, 'repo_manifest_json', repo_manifest_json, 'active_session_ref', active_session_ref, 'snapshot_json', snapshot_json, 'updated_at_ms', updated_at_ms) FROM graphcode_artifacts WHERE registry_key = '{registry_key}' LIMIT 1;",
            registry_key = sql_escape(registry_key),
        ))?;
        raw.map(|value| {
            serde_json::from_str::<GraphCodeArtifactRecord>(&value).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_graphcode_record_decode_failed",
                    format!("Failed to decode GraphCode artifact record: {error}"),
                )
            })
        })
        .transpose()
    }

    pub(crate) fn graphcode_session_by_ref(
        &self,
        session_ref: &str,
    ) -> Result<Option<GraphCodeSessionRecord>> {
        let raw = self.scalar_string(&format!(
            "SELECT json_object('session_ref', session_ref, 'registry_key', registry_key, 'substrate_kind', substrate_kind, 'current_focus_json', current_focus_json, 'pinned_nodes_json', pinned_nodes_json, 'recent_traversals_json', recent_traversals_json, 'working_set_refs_json', working_set_refs_json, 'hydrated_excerpts_json', hydrated_excerpts_json, 'path_artifacts_json', path_artifacts_json, 'snapshot_fingerprint', snapshot_fingerprint, 'freshness_state', freshness_state, 'updated_at_ms', updated_at_ms) FROM graphcode_sessions WHERE session_ref = '{session_ref}' LIMIT 1;",
            session_ref = sql_escape(session_ref),
        ))?;
        raw.map(|value| {
            serde_json::from_str::<GraphCodeSessionRecord>(&value).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_graphcode_session_decode_failed",
                    format!("Failed to decode GraphCode session record: {error}"),
                )
            })
        })
        .transpose()
    }

    pub(crate) fn mark_graphcode_artifact_freshness(
        &self,
        project_id: &str,
        substrate_kind: &str,
        freshness_state: &str,
    ) -> Result<()> {
        self.exec(&format!(
            "UPDATE graphcode_artifacts SET freshness_state = '{freshness_state}', updated_at_ms = {updated_at_ms} WHERE project_id = '{project_id}' AND substrate_kind = '{substrate_kind}';",
            freshness_state = sql_escape(freshness_state),
            updated_at_ms = now_ms(),
            project_id = sql_escape(project_id),
            substrate_kind = sql_escape(substrate_kind),
        ))
    }

    pub(crate) fn mark_graphcode_artifact_freshness_by_registry_key(
        &self,
        registry_key: &str,
        freshness_state: &str,
    ) -> Result<()> {
        self.exec(&format!(
            "UPDATE graphcode_artifacts SET freshness_state = '{freshness_state}', updated_at_ms = {updated_at_ms} WHERE registry_key = '{registry_key}';",
            freshness_state = sql_escape(freshness_state),
            updated_at_ms = now_ms(),
            registry_key = sql_escape(registry_key),
        ))
    }

    pub(crate) fn mark_graphcode_artifact_freshness_by_registry_prefix(
        &self,
        registry_prefix: &str,
        freshness_state: &str,
    ) -> Result<()> {
        self.exec(&format!(
            "UPDATE graphcode_artifacts SET freshness_state = '{freshness_state}', updated_at_ms = {updated_at_ms} WHERE registry_key LIKE '{registry_prefix}%';",
            freshness_state = sql_escape(freshness_state),
            updated_at_ms = now_ms(),
            registry_prefix = sql_escape(registry_prefix),
        ))
    }

    pub(crate) fn mark_graphcode_session_freshness_by_registry_key(
        &self,
        registry_key: &str,
        freshness_state: &str,
    ) -> Result<()> {
        self.exec(&format!(
            "UPDATE graphcode_sessions SET freshness_state = '{freshness_state}', updated_at_ms = {updated_at_ms} WHERE registry_key = '{registry_key}';",
            freshness_state = sql_escape(freshness_state),
            updated_at_ms = now_ms(),
            registry_key = sql_escape(registry_key),
        ))
    }

    pub(crate) fn mark_graphcode_session_freshness_by_registry_prefix(
        &self,
        registry_prefix: &str,
        freshness_state: &str,
    ) -> Result<()> {
        self.exec(&format!(
            "UPDATE graphcode_sessions SET freshness_state = '{freshness_state}', updated_at_ms = {updated_at_ms} WHERE registry_key LIKE '{registry_prefix}%';",
            freshness_state = sql_escape(freshness_state),
            updated_at_ms = now_ms(),
            registry_prefix = sql_escape(registry_prefix),
        ))
    }
}

fn sql_nullable(value: &Option<String>) -> String {
    value
        .as_ref()
        .map(|value| format!("'{}'", sql_escape(value)))
        .unwrap_or_else(|| "NULL".to_string())
}
