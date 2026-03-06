use super::*;

impl Registry {
    pub(crate) fn global_system_prompt_path(&self, prompt_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("system_prompts")
            .join(format!("{prompt_id}.json"))
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
}
