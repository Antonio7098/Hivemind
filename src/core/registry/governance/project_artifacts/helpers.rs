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
            .with_hint(
                "Use 'hivemind project governance document list <project>' to inspect available documents",
            ));
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
}
