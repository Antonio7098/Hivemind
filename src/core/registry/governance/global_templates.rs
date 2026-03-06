use super::*;

impl Registry {
    pub(crate) fn global_template_path(&self, template_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("templates")
            .join(format!("{template_id}.json"))
    }

    pub(crate) fn read_global_template_artifact(
        &self,
        template_id: &str,
        origin: &'static str,
    ) -> Result<GlobalTemplateArtifact> {
        let path = self.global_template_path(template_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "template_not_found",
                format!("Global template '{template_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind global template list' to inspect available templates"));
        }
        let artifact = Self::read_governance_json::<GlobalTemplateArtifact>(
            &path,
            "global_template",
            template_id,
            origin,
        )?;
        if artifact.template_id != template_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Template file key mismatch: expected '{template_id}', found '{}'",
                    artifact.template_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }

    /// Creates a global template artifact with strict reference validation.
    pub fn global_template_create(
        &self,
        template_id: &str,
        system_prompt_id: &str,
        skill_ids: &[String],
        document_ids: &[String],
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        let origin = "registry:global_template_create";
        self.ensure_global_governance_layout(origin)?;
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let system_prompt_id =
            Self::validate_governance_identifier(system_prompt_id, "system_prompt_id", origin)?;
        let normalized_skill_ids = Self::normalized_string_list(skill_ids)
            .into_iter()
            .map(|id| Self::validate_governance_identifier(&id, "skill_id", origin))
            .collect::<Result<Vec<_>>>()?;
        let normalized_document_ids = Self::normalized_string_list(document_ids)
            .into_iter()
            .map(|id| Self::validate_governance_identifier(&id, "document_id", origin))
            .collect::<Result<Vec<_>>>()?;

        let _ = self
            .read_global_system_prompt_artifact(&system_prompt_id, origin)
            .map_err(|err| {
                err.with_hint(
                    "Create the referenced system prompt first via 'hivemind global system-prompt create'",
                )
            })?;
        for skill_id in &normalized_skill_ids {
            let _ = self
                .read_global_skill_artifact(skill_id, origin)
                .map_err(|err| {
                    err.with_hint(
                        "Create the referenced skill first via 'hivemind global skill create'",
                    )
                })?;
        }

        let path = self.global_template_path(&template_id);
        if path.exists() {
            return Err(HivemindError::user(
                "template_exists",
                format!("Global template '{template_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global template update <template-id>' to mutate this template",
            ));
        }

        let now = Utc::now();
        let artifact = GlobalTemplateArtifact {
            template_id: template_id.clone(),
            system_prompt_id: system_prompt_id.clone(),
            skill_ids: normalized_skill_ids.clone(),
            document_ids: normalized_document_ids,
            description: description.map(std::string::ToString::to_string),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("template", &template_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalTemplateSummary {
            template_id,
            system_prompt_id,
            skill_ids: normalized_skill_ids,
            document_ids: artifact.document_ids,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Lists global templates.
    pub fn global_template_list(&self) -> Result<Vec<GlobalTemplateSummary>> {
        let origin = "registry:global_template_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in
            Self::governance_json_paths(&self.governance_global_root().join("templates"), origin)?
        {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalTemplateArtifact>(
                &path,
                "global_template",
                key,
                origin,
            )?;
            out.push(GlobalTemplateSummary {
                template_id: artifact.template_id,
                system_prompt_id: artifact.system_prompt_id,
                skill_ids: artifact.skill_ids,
                document_ids: artifact.document_ids,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.template_id.cmp(&b.template_id));
        Ok(out)
    }

    /// Inspects a global template.
    pub fn global_template_inspect(
        &self,
        template_id: &str,
    ) -> Result<GlobalTemplateInspectResult> {
        let origin = "registry:global_template_inspect";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        let artifact = self.read_global_template_artifact(&template_id, origin)?;

        Ok(GlobalTemplateInspectResult {
            summary: GlobalTemplateSummary {
                template_id,
                system_prompt_id: artifact.system_prompt_id,
                skill_ids: artifact.skill_ids,
                document_ids: artifact.document_ids,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            description: artifact.description,
        })
    }

    /// Updates a global template with strict reference validation.
    pub fn global_template_update(
        &self,
        template_id: &str,
        system_prompt_id: Option<&str>,
        skill_ids: Option<&[String]>,
        document_ids: Option<&[String]>,
        description: Option<&str>,
    ) -> Result<GlobalTemplateSummary> {
        let origin = "registry:global_template_update";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        let mut artifact = self.read_global_template_artifact(&template_id, origin)?;
        let mut changed = false;

        if let Some(system_prompt_id) = system_prompt_id {
            let system_prompt_id =
                Self::validate_governance_identifier(system_prompt_id, "system_prompt_id", origin)?;
            let _ = self
                .read_global_system_prompt_artifact(&system_prompt_id, origin)
                .map_err(|err| {
                    err.with_hint(
                        "Create the referenced system prompt first via 'hivemind global system-prompt create'",
                    )
                })?;
            if artifact.system_prompt_id != system_prompt_id {
                artifact.system_prompt_id = system_prompt_id;
                changed = true;
            }
        }

        if let Some(skill_ids) = skill_ids {
            let normalized = Self::normalized_string_list(skill_ids)
                .into_iter()
                .map(|id| Self::validate_governance_identifier(&id, "skill_id", origin))
                .collect::<Result<Vec<_>>>()?;
            for skill_id in &normalized {
                let _ = self
                    .read_global_skill_artifact(skill_id, origin)
                    .map_err(|err| {
                        err.with_hint(
                            "Create the referenced skill first via 'hivemind global skill create'",
                        )
                    })?;
            }
            if artifact.skill_ids != normalized {
                artifact.skill_ids = normalized;
                changed = true;
            }
        }

        if let Some(document_ids) = document_ids {
            let normalized = Self::normalized_string_list(document_ids)
                .into_iter()
                .map(|id| Self::validate_governance_identifier(&id, "document_id", origin))
                .collect::<Result<Vec<_>>>()?;
            if artifact.document_ids != normalized {
                artifact.document_ids = normalized;
                changed = true;
            }
        }

        if let Some(description) = description {
            let next = if description.trim().is_empty() {
                None
            } else {
                Some(description.to_string())
            };
            if artifact.description != next {
                artifact.description = next;
                changed = true;
            }
        }

        if changed {
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location = Self::governance_global_location("template", &template_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalTemplateSummary {
            template_id,
            system_prompt_id: artifact.system_prompt_id,
            skill_ids: artifact.skill_ids,
            document_ids: artifact.document_ids,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Deletes a global template.
    pub fn global_template_delete(
        &self,
        template_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_template_delete";
        let template_id = Self::validate_governance_identifier(template_id, "template_id", origin)?;
        let path = self.global_template_path(&template_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "template_not_found",
                format!("Global template '{template_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;
        let location = Self::governance_global_location("template", &template_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "template".to_string(),
            artifact_key: template_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

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
