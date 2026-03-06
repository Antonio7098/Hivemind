use super::*;

impl Registry {
    /// Sets attachment state for a project document on a task.
    pub fn project_governance_attachment_set_document(
        &self,
        id_or_name: &str,
        task_id: &str,
        document_id: &str,
        attached: bool,
    ) -> Result<GovernanceAttachmentUpdateResult> {
        let origin = "registry:project_governance_attachment_set_document";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let task = self.get_task(task_id).inspect_err(|err| {
            self.record_error_event(err, CorrelationIds::for_project(project.id));
        })?;
        if task.project_id != project.id {
            return Err(HivemindError::user(
                "task_project_mismatch",
                format!(
                    "Task '{}' does not belong to project '{}'",
                    task.id, project.id
                ),
                origin,
            )
            .with_hint("Pass a task ID that belongs to the selected project"));
        }
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let _ = self.read_project_document_artifact(project.id, &document_id, origin)?;

        self.append_event(
            Event::new(
                EventPayload::GovernanceAttachmentLifecycleUpdated {
                    project_id: project.id,
                    task_id: task.id,
                    artifact_kind: "document".to_string(),
                    artifact_key: document_id.clone(),
                    attached,
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_task(project.id, task.id),
            ),
            origin,
        )?;

        Ok(GovernanceAttachmentUpdateResult {
            project_id: project.id,
            task_id: task.id,
            artifact_kind: "document".to_string(),
            artifact_key: document_id,
            attached,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }
}
