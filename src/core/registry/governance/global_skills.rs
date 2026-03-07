use super::*;

impl Registry {
    pub(crate) fn global_skill_path(&self, skill_id: &str) -> PathBuf {
        self.governance_global_root()
            .join("skills")
            .join(format!("{skill_id}.json"))
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
}
