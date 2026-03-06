use super::*;

impl Registry {
    pub(crate) fn project_document_path(&self, project_id: Uuid, document_id: &str) -> PathBuf {
        self.governance_project_root(project_id)
            .join("documents")
            .join(format!("{document_id}.json"))
    }

    pub(crate) fn governance_document_location(
        &self,
        project_id: Uuid,
        document_id: &str,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "document",
            artifact_key: document_id.to_string(),
            is_dir: false,
            path: self.project_document_path(project_id, document_id),
        }
    }

    pub(crate) fn governance_notepad_location(
        &self,
        project_id: Option<Uuid>,
    ) -> GovernanceArtifactLocation {
        project_id.map_or_else(
            || GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: self.governance_global_root().join("notepad.md"),
            },
            |project_id| GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: self.governance_project_root(project_id).join("notepad.md"),
            },
        )
    }

    pub(crate) fn read_project_document_artifact(
        &self,
        project_id: Uuid,
        document_id: &str,
        origin: &'static str,
    ) -> Result<ProjectDocumentArtifact> {
        let path = self.project_document_path(project_id, document_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "document_not_found",
                format!("Project document '{document_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind project governance document list <project>' to inspect available documents"));
        }
        let artifact = Self::read_governance_json::<ProjectDocumentArtifact>(
            &path,
            "project_document",
            document_id,
            origin,
        )?;
        if artifact.document_id != document_id {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!(
                    "Document file key mismatch: expected '{document_id}', found '{}'",
                    artifact.document_id
                ),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string()));
        }
        if artifact.revisions.is_empty() {
            return Err(HivemindError::user(
                "governance_artifact_schema_invalid",
                format!("Document '{document_id}' has no revision history"),
                origin,
            )
            .with_hint("Repair the document JSON to include at least one revision"));
        }
        Ok(artifact)
    }

    pub(crate) fn governance_json_paths(dir: &Path, origin: &'static str) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        if !dir.is_dir() {
            return Ok(paths);
        }
        for entry in fs::read_dir(dir).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", dir.to_string_lossy().to_string())
        })? {
            let entry = entry.map_err(|e| {
                HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                    .with_context("path", dir.to_string_lossy().to_string())
            })?;
            let path = entry.path();
            if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }

    pub(crate) fn project_document_summary_from_artifact(
        project_id: Uuid,
        artifact: &ProjectDocumentArtifact,
        path: &Path,
    ) -> ProjectGovernanceDocumentSummary {
        let revision = artifact.revisions.last().map_or(0, |rev| rev.revision);
        ProjectGovernanceDocumentSummary {
            project_id,
            document_id: artifact.document_id.clone(),
            title: artifact.title.clone(),
            owner: artifact.owner.clone(),
            tags: artifact.tags.clone(),
            updated_at: artifact.updated_at,
            revision,
            path: path.to_string_lossy().to_string(),
        }
    }

    /// Creates a project governance document with immutable revision history.
    pub fn project_governance_document_create(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: &str,
        owner: &str,
        tags: &[String],
        content: &str,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        let origin = "registry:project_governance_document_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        if title.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_title",
                "Document title cannot be empty",
                origin,
            )
            .with_hint("Pass --title with a non-empty value"));
        }
        if owner.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_owner",
                "Document owner cannot be empty",
                origin,
            )
            .with_hint("Pass --owner with a non-empty value"));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_document_content",
                "Document content cannot be empty",
                origin,
            )
            .with_hint("Pass --content with a non-empty value"));
        }

        self.ensure_governance_layout(project.id, origin)?;
        let path = self.project_document_path(project.id, &document_id);
        if path.exists() {
            return Err(HivemindError::user(
                "document_exists",
                format!("Project document '{document_id}' already exists"),
                origin,
            )
            .with_hint(
                "Use 'hivemind project governance document update' to create a new revision",
            ));
        }

        let now = Utc::now();
        let artifact = ProjectDocumentArtifact {
            document_id: document_id.clone(),
            title: title.trim().to_string(),
            owner: owner.trim().to_string(),
            tags: Self::normalized_string_list(tags),
            updated_at: now,
            revisions: vec![ProjectDocumentRevision {
                revision: 1,
                content: content.to_string(),
                updated_at: now,
            }],
        };
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = self.governance_document_location(project.id, &document_id);
        let event_revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(ProjectGovernanceDocumentWriteResult {
            project_id: project.id,
            document_id,
            revision: event_revision,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            updated_at: now,
        })
    }

    /// Lists project governance documents.
    pub fn project_governance_document_list(
        &self,
        id_or_name: &str,
    ) -> Result<Vec<ProjectGovernanceDocumentSummary>> {
        let origin = "registry:project_governance_document_list";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        let mut out = Vec::new();
        for path in Self::governance_json_paths(
            &self.governance_project_root(project.id).join("documents"),
            origin,
        )? {
            let key = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("unknown");
            let artifact = Self::read_governance_json::<ProjectDocumentArtifact>(
                &path,
                "project_document",
                key,
                origin,
            )?;
            out.push(Self::project_document_summary_from_artifact(
                project.id, &artifact, &path,
            ));
        }
        out.sort_by(|a, b| a.document_id.cmp(&b.document_id));
        Ok(out)
    }

    /// Inspects a project governance document with full revision history.
    pub fn project_governance_document_inspect(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<ProjectGovernanceDocumentInspectResult> {
        let origin = "registry:project_governance_document_inspect";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let path = self.project_document_path(project.id, &document_id);
        let artifact = self.read_project_document_artifact(project.id, &document_id, origin)?;
        let summary = Self::project_document_summary_from_artifact(project.id, &artifact, &path);
        let latest_content = artifact
            .revisions
            .last()
            .map_or_else(String::new, |item| item.content.clone());

        Ok(ProjectGovernanceDocumentInspectResult {
            summary,
            revisions: artifact.revisions,
            latest_content,
        })
    }

    /// Updates a project governance document, optionally creating a new immutable revision.
    pub fn project_governance_document_update(
        &self,
        id_or_name: &str,
        document_id: &str,
        title: Option<&str>,
        owner: Option<&str>,
        tags: Option<&[String]>,
        content: Option<&str>,
    ) -> Result<ProjectGovernanceDocumentWriteResult> {
        let origin = "registry:project_governance_document_update";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let mut artifact = self.read_project_document_artifact(project.id, &document_id, origin)?;
        let path = self.project_document_path(project.id, &document_id);

        let mut changed = false;
        if let Some(title) = title {
            if title.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_title",
                    "Document title cannot be empty",
                    origin,
                )
                .with_hint("Pass --title with a non-empty value"));
            }
            if artifact.title != title.trim() {
                artifact.title = title.trim().to_string();
                changed = true;
            }
        }
        if let Some(owner) = owner {
            if owner.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_owner",
                    "Document owner cannot be empty",
                    origin,
                )
                .with_hint("Pass --owner with a non-empty value"));
            }
            if artifact.owner != owner.trim() {
                artifact.owner = owner.trim().to_string();
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

        let now = Utc::now();
        if let Some(content) = content {
            if content.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_document_content",
                    "Document content cannot be empty",
                    origin,
                )
                .with_hint("Pass --content with a non-empty value"));
            }
            let next_revision = artifact
                .revisions
                .last()
                .map_or(1, |item| item.revision.saturating_add(1));
            artifact.revisions.push(ProjectDocumentRevision {
                revision: next_revision,
                content: content.to_string(),
                updated_at: now,
            });
            changed = true;
        }

        if !changed {
            let revision = artifact.revisions.last().map_or(0, |item| item.revision);
            return Ok(ProjectGovernanceDocumentWriteResult {
                project_id: project.id,
                document_id,
                revision,
                path: path.to_string_lossy().to_string(),
                schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                projection_version: GOVERNANCE_PROJECTION_VERSION,
                updated_at: artifact.updated_at,
            });
        }

        artifact.updated_at = now;
        Self::write_governance_json(&path, &artifact, origin)?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let location = self.governance_document_location(project.id, &document_id);
        let event_revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(ProjectGovernanceDocumentWriteResult {
            project_id: project.id,
            document_id,
            revision: event_revision,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            updated_at: now,
        })
    }

    /// Deletes a project governance document.
    pub fn project_governance_document_delete(
        &self,
        id_or_name: &str,
        document_id: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:project_governance_document_delete";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let document_id = Self::validate_governance_identifier(document_id, "document_id", origin)?;
        let path = self.project_document_path(project.id, &document_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "document_not_found",
                format!("Project document '{document_id}' not found"),
                origin,
            )
            .with_hint("Use 'hivemind project governance document list <project>' to inspect available documents"));
        }

        fs::remove_file(&path).map_err(|e| {
            HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;

        let location = self.governance_document_location(project.id, &document_id);
        self.append_governance_delete_for_location(
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: Some(project.id),
            scope: "project".to_string(),
            artifact_kind: "document".to_string(),
            artifact_key: document_id,
            path: path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

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

    /// Creates project notepad content (non-executional, non-validating artifact).
    pub fn project_governance_notepad_create(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        let location = self.governance_notepad_location(Some(project.id));
        let existing = fs::read_to_string(&location.path).unwrap_or_default();
        if !existing.trim().is_empty() {
            return Err(HivemindError::user(
                "notepad_exists",
                "Project notepad already has content",
                origin,
            )
            .with_hint(
                "Use 'hivemind project governance notepad update <project>' to replace content",
            ));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
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
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Shows project notepad content.
    pub fn project_governance_notepad_show(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_show";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_notepad_location(Some(project.id));
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
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists,
            content,
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Updates project notepad content.
    pub fn project_governance_notepad_update(
        &self,
        id_or_name: &str,
        content: &str,
    ) -> Result<GovernanceNotepadResult> {
        let origin = "registry:project_governance_notepad_update";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_notepad_content",
                "Notepad content cannot be empty",
                origin,
            ));
        }
        let location = self.governance_notepad_location(Some(project.id));
        fs::write(&location.path, content).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })?;

        let state = self.state()?;
        let mut pending = HashMap::new();
        let _ = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceNotepadResult {
            scope: "project".to_string(),
            project_id: Some(project.id),
            path: location.path.to_string_lossy().to_string(),
            exists: true,
            content: Some(content.to_string()),
            non_executional: true,
            non_validating: true,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Deletes project notepad content.
    pub fn project_governance_notepad_delete(
        &self,
        id_or_name: &str,
    ) -> Result<GovernanceArtifactDeleteResult> {
        let origin = "registry:project_governance_notepad_delete";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_notepad_location(Some(project.id));
        if location.path.exists() {
            fs::remove_file(&location.path).map_err(|e| {
                HivemindError::system("governance_artifact_delete_failed", e.to_string(), origin)
            })?;
        }
        self.append_governance_delete_for_location(
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;

        Ok(GovernanceArtifactDeleteResult {
            project_id: Some(project.id),
            scope: "project".to_string(),
            artifact_kind: "notepad".to_string(),
            artifact_key: "notepad.md".to_string(),
            path: location.path.to_string_lossy().to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }
}
