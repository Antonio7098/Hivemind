use super::*;

impl Registry {
    pub fn governance_global_root(&self) -> PathBuf {
        self.config.data_dir.join("global")
    }

    pub(crate) fn ensure_global_governance_layout(&self, origin: &'static str) -> Result<()> {
        let global_root = self.governance_global_root();
        fs::create_dir_all(global_root.join("skills")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        fs::create_dir_all(global_root.join("system_prompts")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        fs::create_dir_all(global_root.join("templates")).map_err(|e| {
            HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
        })?;
        let notepad = global_root.join("notepad.md");
        if !notepad.exists() {
            fs::write(&notepad, b"\n").map_err(|e| {
                HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
            })?;
        }
        Ok(())
    }

    pub(crate) fn global_skill_path(&self, skill_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("skills")
            .join(format!("{skill_id}.json"))
    }

    pub(crate) fn global_system_prompt_path(&self, prompt_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("system_prompts")
            .join(format!("{prompt_id}.json"))
    }

    pub(crate) fn global_template_path(&self, template_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("templates")
            .join(format!("{template_id}.json"))
    }

    pub(crate) fn governance_global_location(
        artifact_kind: &'static str,
        artifact_key: &str,
        path: PathBuf,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: None,
            scope: "global",
            artifact_kind,
            artifact_key: artifact_key.to_string(),
            is_dir: false,
            path,
        }
    }

    pub(crate) fn read_global_skill_artifact(
        &self,
        skill_id: &str,
        origin: &'static str,
    ) -> Result<GlobalSkillArtifact> {
        let path = self.global_skill_path(skill_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "skill_not_found",
                format!("Global skill '{skill_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind global skill list' to inspect available skills"));
        }
        let artifact = Self::read_governance_json::<GlobalSkillArtifact>(
            &path,
            "global_skill",
            skill_id,
            origin,
        )?;
        if artifact.skill_id != skill_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Skill file key mismatch: expected '{skill_id}', found '{}'",
                    artifact.skill_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
    }

    pub(crate) fn read_global_system_prompt_artifact(
        &self,
        prompt_id: &str,
        origin: &'static str,
    ) -> Result<GlobalSystemPromptArtifact> {
        let path = self.global_system_prompt_path(prompt_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "system_prompt_not_found",
                format!("Global system prompt '{prompt_id}' not found"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global system-prompt list' to inspect available system prompts",
            ));
        }
        let artifact = Self::read_governance_json::<GlobalSystemPromptArtifact>(
            &path,
            "global_system_prompt",
            prompt_id,
            origin,
        )?;
        if artifact.prompt_id != prompt_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "System prompt file key mismatch: expected '{prompt_id}', found '{}'",
                    artifact.prompt_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        Ok(artifact)
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

    /// Creates a global skill artifact.
    pub fn global_skill_create(
        &self,
        skill_id: &str,
        name: &str,
        tags: &[String],
        content: &str,
    ) -> Result<GlobalSkillSummary> {
        let origin = "registry:global_skill_create";
        self.ensure_global_governance_layout(origin)?;
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        if name.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_skill_name",
                "Skill name cannot be empty",
                origin,
            ));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_skill_content",
                "Skill content cannot be empty",
                origin,
            ));
        }

        let path = self.global_skill_path(&skill_id);
        if path.exists() {
            return Err(HivemindError::user(
                "skill_exists",
                format!("Global skill '{skill_id}' already exists"),
                origin,
            )
            .with_hint("Use 'hivemind global skill update <skill-id>' to mutate this skill"));
        }

        let now = Utc::now();
        let artifact = GlobalSkillArtifact {
            skill_id: skill_id.clone(),
            name: name.trim().to_string(),
            tags: Self::normalized_string_list(tags),
            content: content.to_string(),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("skill", &skill_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalSkillSummary {
            skill_id,
            name: artifact.name,
            tags: artifact.tags,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Lists global skills.
    pub fn global_skill_list(&self) -> Result<Vec<GlobalSkillSummary>> {
        let origin = "registry:global_skill_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in
            Self::governance_json_paths(&self.governance_global_root().join("skills"), origin)?
        {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalSkillArtifact>(
                &path,
                "global_skill",
                key,
                origin,
            )?;
            out.push(GlobalSkillSummary {
                skill_id: artifact.skill_id,
                name: artifact.name,
                tags: artifact.tags,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        Ok(out)
    }

    /// Inspects a global skill.
    pub fn global_skill_inspect(&self, skill_id: &str) -> Result<GlobalSkillInspectResult> {
        let origin = "registry:global_skill_inspect";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        let artifact = self.read_global_skill_artifact(&skill_id, origin)?;
        Ok(GlobalSkillInspectResult {
            summary: GlobalSkillSummary {
                skill_id,
                name: artifact.name,
                tags: artifact.tags,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            content: artifact.content,
        })
    }

    /// Updates a global skill.
    pub fn global_skill_update(
        &self,
        skill_id: &str,
        name: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<GlobalSkillSummary> {
        let origin = "registry:global_skill_update";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        let mut artifact = self.read_global_skill_artifact(&skill_id, origin)?;
        let mut changed = false;

        if let Some(name) = name {
            if name.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_skill_name",
                    "Skill name cannot be empty",
                    origin,
                ));
            }
            if artifact.name != name.trim() {
                artifact.name = name.trim().to_string();
                changed = true;
            }
        }
        if let Some(tags) = tags {
            let normalized = Self::normalized_string_list(tags);
            if artifact.tags != normalized {
                artifact.tags = normalized;
                changed = true;
            }
        }
        if let Some(content) = content {
            if content.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_skill_content",
                    "Skill content cannot be empty",
                    origin,
                ));
            }
            if artifact.content != content {
                artifact.content = content.to_string();
                changed = true;
            }
        }

        if changed {
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location = Self::governance_global_location("skill", &skill_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalSkillSummary {
            skill_id,
            name: artifact.name,
            tags: artifact.tags,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Deletes a global skill.
    pub fn global_skill_delete(&self, skill_id: &str) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_skill_delete";
        let skill_id = Self::validate_governance_identifier(skill_id, "skill_id", origin)?;
        let path = self.global_skill_path(&skill_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "skill_not_found",
                format!("Global skill '{skill_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;

        let location = Self::governance_global_location("skill", &skill_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "skill".to_string(),
            artifact_key: skill_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Creates a global system prompt.
    pub fn global_system_prompt_create(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        let origin = "registry:global_system_prompt_create";
        self.ensure_global_governance_layout(origin)?;
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_system_prompt_content",
                "System prompt content cannot be empty",
                origin,
            ));
        }

        let path = self.global_system_prompt_path(&prompt_id);
        if path.exists() {
            return Err(HivemindError::user(
                "system_prompt_exists",
                format!("Global system prompt '{prompt_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind global system-prompt update <prompt-id>' to mutate this prompt",
            ));
        }

        let now = Utc::now();
        let artifact = GlobalSystemPromptArtifact {
            prompt_id: prompt_id.clone(),
            content: content.to_string(),
            updated_at: now,
        };
        Self::write_governance_json(&path, &artifact, origin)?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = Self::governance_global_location("system_prompt", &prompt_id, path.clone());
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GlobalSystemPromptSummary {
            prompt_id,
            updated_at: now,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Lists global system prompts.
    pub fn global_system_prompt_list(&self) -> Result<Vec<GlobalSystemPromptSummary>> {
        let origin = "registry:global_system_prompt_list";
        self.ensure_global_governance_layout(origin)?;
        let mut out = Vec::new();
        for path in Self::governance_json_paths(
            &self.governance_global_root().join("system_prompts"),
            origin,
        )? {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<GlobalSystemPromptArtifact>(
                &path,
                "global_system_prompt",
                key,
                origin,
            )?;
            out.push(GlobalSystemPromptSummary {
                prompt_id: artifact.prompt_id,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            });
        }
        out.sort_by(|a, b| a.prompt_id.cmp(&b.prompt_id));
        Ok(out)
    }

    /// Inspects a global system prompt.
    pub fn global_system_prompt_inspect(
        &self,
        prompt_id: &str,
    ) -> Result<GlobalSystemPromptInspectResult> {
        let origin = "registry:global_system_prompt_inspect";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        let path = self.global_system_prompt_path(&prompt_id);
        let artifact = self.read_global_system_prompt_artifact(&prompt_id, origin)?;
        Ok(GlobalSystemPromptInspectResult {
            summary: GlobalSystemPromptSummary {
                prompt_id,
                updated_at: artifact.updated_at,
                path: path.to_string_lossy().to_string(),
            },
            content: artifact.content,
        })
    }

    /// Updates a global system prompt.
    pub fn global_system_prompt_update(
        &self,
        prompt_id: &str,
        content: &str,
    ) -> Result<GlobalSystemPromptSummary> {
        let origin = "registry:global_system_prompt_update";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_system_prompt_content",
                "System prompt content cannot be empty",
                origin,
            ));
        }

        let path = self.global_system_prompt_path(&prompt_id);
        let mut artifact = self.read_global_system_prompt_artifact(&prompt_id, origin)?;
        if artifact.content != content {
            artifact.content = content.to_string();
            artifact.updated_at = Utc::now();
            Self::write_governance_json(&path, &artifact, origin)?;
            let state = self.state()?;
            let mut pending = HashMap::new();
            let location =
                Self::governance_global_location("system_prompt", &prompt_id, path.clone());
            let _ = self.append_governance_upsert_for_location(
                &state,
                &mut pending,
                &location,
                CorrelationIds::none(),
                origin,
            )?;
        }

        Ok(GlobalSystemPromptSummary {
            prompt_id,
            updated_at: artifact.updated_at,
            path: path.to_string_lossy().to_string(),
        })
    }

    /// Deletes a global system prompt.
    pub fn global_system_prompt_delete(
        &self,
        prompt_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_system_prompt_delete";
        let prompt_id = Self::validate_governance_identifier(prompt_id, "prompt_id", origin)?;
        let path = self.global_system_prompt_path(&prompt_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "system_prompt_not_found",
                format!("Global system prompt '{prompt_id}' not found"),
                origin,
            ));
        }
        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
        })?;
        let location = Self::governance_global_location("system_prompt", &prompt_id, path.clone());
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "system_prompt".to_string(),
            artifact_key: prompt_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
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

    /// Creates global notepad content.
    pub fn global_notepad_create(&self, content: &str) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_create";
        self.ensure_global_governance_layout(origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }

        let location = self.governance_notepad_location(None);
        let existing = fs::read_to_string(&location.path).unwrap_or_default();
        if !existing.trim().is_empty() {
            return Err(HivemindError::user(
                "notepad_exists",
                "Global notepad already has content",
                origin,
            )
            .with_hint("Use 'hivemind global notepad update' to replace content"));
        }

        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Shows global notepad content.
    pub fn global_notepad_show(&self) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_show";
        self.ensure_global_governance_layout(origin)?;
        let location = self.governance_notepad_location(None);
        let raw_content = if location.path.is_file() {
            Some(fs::read_to_string(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
            })?)
        } else {
            None
        };
        let (exists, content) = match raw_content {
            Some(content) if content.trim().is_empty() => (false, None),
            Some(content) => (true, Some(content)),
            None => (false, None),
        };
        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists,
            content,
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Updates global notepad content.
    pub fn global_notepad_update(&self, content: &str) -> Result<GovernanceNotepadResult> {
        let origin = "registry:global_notepad_update";
        self.ensure_global_governance_layout(origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }
        let location = self.governance_notepad_location(None);
        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;
        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::none(),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "global".to_string(),
            project_id: None,
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Deletes global notepad content.
    pub fn global_notepad_delete(&self) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:global_notepad_delete";
        self.ensure_global_governance_layout(origin)?;
        let location = self.governance_notepad_location(None);
        if location.path.exists() {
            fs::remove_file(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
            })?;
        }
        self.append_governance_delete_for_location(&location, CorrelationIds::none(), origin)?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: None,
            scope: "global".to_string(),
            artifact_kind: "notepad".to_string(),
            artifact_key: "notepad.md".to_string(),
            path: location.path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }
}
