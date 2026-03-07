use super::*;

impl Registry {
    pub(crate) fn governance_document_attachment_states(
        state: &AppState,
        project_id: Uuid,
        task_id: Uuid,
    ) -> (Vec<String>, Vec<String>) {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        for item in state.governance_attachments.values().filter(|item| {
            item.project_id == project_id
                && item.task_id == task_id
                && item.artifact_kind == "document"
        }) {
            if item.attached {
                included.push(item.artifact_key.clone());
            } else {
                excluded.push(item.artifact_key.clone());
            }
        }
        included.sort();
        included.dedup();
        excluded.sort();
        excluded.dedup();
        (included, excluded)
    }

    pub(crate) fn governance_artifact_revision(
        state: &AppState,
        project_id: Option<Uuid>,
        scope: &str,
        artifact_kind: &str,
        artifact_key: &str,
    ) -> Option<u64> {
        state
            .governance_artifacts
            .values()
            .find(|item| {
                item.project_id == project_id
                    && item.scope == scope
                    && item.artifact_kind == artifact_kind
                    && item.artifact_key == artifact_key
            })
            .map(|item| item.revision)
    }

    pub(crate) fn latest_template_instantiation_snapshot(
        &self,
        project_id: Uuid,
    ) -> Result<Option<TemplateInstantiationSnapshot>> {
        let events = self.read_events(&EventFilter::for_project(project_id))?;
        let mut snapshot: Option<TemplateInstantiationSnapshot> = None;
        for event in events {
            let event_id = event.id().as_uuid().to_string();
            if let EventPayload::TemplateInstantiated {
                project_id: event_project_id,
                template_id,
                system_prompt_id,
                skill_ids,
                document_ids,
                schema_version,
                projection_version,
            } = event.payload
            {
                if event_project_id != project_id {
                    continue;
                }
                snapshot = Some(TemplateInstantiationSnapshot {
                    event_id,
                    template_id,
                    system_prompt_id,
                    skill_ids,
                    document_ids,
                    schema_version,
                    projection_version,
                });
            }
        }
        Ok(snapshot)
    }

    pub(crate) fn attempt_manifest_hash_for_attempt(
        &self,
        attempt_id: Uuid,
    ) -> Result<Option<String>> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        let mut manifest_hash = None;
        for event in events {
            if let EventPayload::AttemptContextAssembled {
                manifest_hash: hash,
                ..
            } = event.payload
            {
                manifest_hash = Some(hash);
            }
        }
        Ok(manifest_hash)
    }

    pub(crate) fn governance_projection_map_for_project(
        state: &AppState,
        project_id: Uuid,
    ) -> BTreeMap<String, crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .iter()
            .filter(|(_, artifact)| {
                artifact.project_id == Some(project_id) || artifact.project_id.is_none()
            })
            .map(|(key, artifact)| (key.clone(), artifact.clone()))
            .collect()
    }
}
