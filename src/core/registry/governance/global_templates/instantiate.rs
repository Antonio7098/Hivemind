use super::*;

impl Registry {
    /// Resolves a global template for a specific project and emits an instantiation event.
    pub fn global_template_instantiate(
        &self,
        id_or_name: &str,
        template_id: &str,
    ) -> Result<TemplateInstantiationResult> {
        let origin = "registry:global_template_instantiate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let artifact = self.read_global_template_artifact(&template_id, origin)?;
        let _ = self
            .read_global_system_prompt_artifact(&artifact.system_prompt_id, origin)
            .map_err(|err| {
                err.with_hint(
                    "Fix template.system_prompt_id or recreate the missing system prompt before instantiation",
                )
            })?;

        for skill_id in &artifact.skill_ids {
            let _ = self
                .read_global_skill_artifact(skill_id, origin)
                .map_err(|err| {
                    err.with_hint("Fix template.skill_ids or recreate the missing global skill")
                })?;
        }
        for document_id in &artifact.document_ids {
            let _ = self
                .read_project_document_artifact(project.id, document_id, origin)
                .map_err(|err| {
                    err.with_hint(
                        "Fix template.document_ids or create the missing project document before instantiation",
                    )
                })?;
        }

        let now = Utc::now();
        self.append_event(
            Event::new(
                EventPayload::TemplateInstantiated {
                    project_id: project.id,
                    template_id: template_id.clone(),
                    system_prompt_id: artifact.system_prompt_id.clone(),
                    skill_ids: artifact.skill_ids.clone(),
                    document_ids: artifact.document_ids.clone(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(TemplateInstantiationResult {
            project_id: project.id,
            template_id,
            system_prompt_id: artifact.system_prompt_id,
            skill_ids: artifact.skill_ids,
            document_ids: artifact.document_ids,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            instantiated_at: now,
        })
    }
}
