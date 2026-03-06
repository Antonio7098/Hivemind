use super::*;

impl Registry {
    pub(crate) fn governance_artifact_projection_key(
        location: &GovernanceArtifactLocation,
    ) -> String {
        format!(
            "{}::{}::{}::{}",
            location
                .project_id
                .map_or_else(|| "global".to_string(), |id| id.to_string()),
            location.scope,
            location.artifact_kind,
            location.artifact_key
        )
    }

    pub(crate) fn governance_projection_for_location<'a>(
        state: &'a AppState,
        location: &GovernanceArtifactLocation,
    ) -> Option<&'a crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .get(&Self::governance_artifact_projection_key(location))
    }

    pub(crate) fn next_governance_revision(
        state: &AppState,
        location: &GovernanceArtifactLocation,
        pending_revisions: &mut HashMap<String, u64>,
    ) -> u64 {
        let key = Self::governance_artifact_projection_key(location);
        if let Some(next) = pending_revisions.get_mut(&key) {
            *next = next.saturating_add(1);
            return *next;
        }
        let next = Self::governance_projection_for_location(state, location)
            .map_or(1, |existing| existing.revision.saturating_add(1));
        pending_revisions.insert(key, next);
        next
    }

    pub(crate) fn governance_projection_key_from_parts(
        project_id: Option<Uuid>,
        scope: &str,
        artifact_kind: &str,
        artifact_key: &str,
    ) -> String {
        let project_key = project_id.map_or_else(|| "global".to_string(), |id| id.to_string());
        format!("{project_key}::{scope}::{artifact_kind}::{artifact_key}")
    }

    pub(crate) fn governance_location_for_path(
        &self,
        project_id: Uuid,
        path: &Path,
    ) -> Option<GovernanceArtifactLocation> {
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();
        let documents_root = project_root.join("documents");
        let skills_root = global_root.join("skills");
        let prompts_root = global_root.join("system_prompts");
        let templates_root = global_root.join("templates");

        if path == self.constitution_path(project_id) {
            return Some(self.governance_constitution_location(project_id));
        }
        if path == self.graph_snapshot_path(project_id) {
            return Some(self.governance_graph_snapshot_location(project_id));
        }
        if path == self.governance_notepad_location(Some(project_id)).path {
            return Some(self.governance_notepad_location(Some(project_id)));
        }
        if path == self.governance_notepad_location(None).path {
            return Some(self.governance_notepad_location(None));
        }

        if path.starts_with(&documents_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(self.governance_document_location(project_id, &key));
        }
        if path.starts_with(&skills_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "skill",
                &key,
                path.to_path_buf(),
            ));
        }
        if path.starts_with(&prompts_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "system_prompt",
                &key,
                path.to_path_buf(),
            ));
        }
        if path.starts_with(&templates_root) && path.extension().is_some_and(|ext| ext == "json") {
            let key = path.file_stem()?.to_str()?.to_string();
            return Some(Self::governance_global_location(
                "template",
                &key,
                path.to_path_buf(),
            ));
        }

        None
    }

    pub(crate) fn governance_managed_file_locations(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<GovernanceArtifactLocation>> {
        let mut out = Vec::new();
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();

        let constitution = self.governance_constitution_location(project_id);
        if constitution.path.is_file() {
            out.push(constitution);
        }
        let graph_snapshot = self.governance_graph_snapshot_location(project_id);
        if graph_snapshot.path.is_file() {
            out.push(graph_snapshot);
        }
        let project_notepad = self.governance_notepad_location(Some(project_id));
        if project_notepad.path.is_file() {
            out.push(project_notepad);
        }
        let global_notepad = self.governance_notepad_location(None);
        if global_notepad.path.is_file() {
            out.push(global_notepad);
        }

        for path in Self::governance_json_paths(&project_root.join("documents"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("skills"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("system_prompts"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }
        for path in Self::governance_json_paths(&global_root.join("templates"), origin)? {
            if let Some(location) = self.governance_location_for_path(project_id, &path) {
                out.push(location);
            }
        }

        out.sort_by(|a, b| {
            a.scope
                .cmp(b.scope)
                .then(a.artifact_kind.cmp(b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
                .then(a.path.cmp(&b.path))
        });
        Ok(out)
    }

    pub(crate) fn append_governance_upsert_for_location(
        &self,
        state: &AppState,
        pending_revisions: &mut HashMap<String, u64>,
        location: &GovernanceArtifactLocation,
        corr: CorrelationIds,
        origin: &'static str,
    ) -> Result<u64> {
        let revision = Self::next_governance_revision(state, location, pending_revisions);
        self.append_event(
            Event::new(
                EventPayload::GovernanceArtifactUpserted {
                    project_id: location.project_id,
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key.clone(),
                    path: location.path.to_string_lossy().to_string(),
                    revision,
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                corr,
            ),
            origin,
        )?;
        Ok(revision)
    }

    pub(crate) fn append_governance_delete_for_location(
        &self,
        location: &GovernanceArtifactLocation,
        corr: CorrelationIds,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::GovernanceArtifactDeleted {
                    project_id: location.project_id,
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key.clone(),
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                corr,
            ),
            origin,
        )
    }
}
