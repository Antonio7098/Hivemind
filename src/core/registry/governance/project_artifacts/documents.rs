use super::*;

impl Registry {
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
            .with_hint(
                "Use 'hivemind project governance document list <project>' to inspect available documents",
            ));
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
}
