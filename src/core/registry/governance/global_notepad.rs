use super::*;

impl Registry {
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
