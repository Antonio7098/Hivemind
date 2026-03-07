use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_diagnose(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceDiagnosticsResult> {
        let origin = "registry:project_governance_diagnose";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut issues: Vec<GovernanceDiagnosticIssue> = Vec::new();
        let mut push_issue = |issue: GovernanceDiagnosticIssue| {
            issues.push(issue);
        };

        let constitution_path = self.constitution_path(project.id);
        match self.read_constitution_artifact(project.id, origin) {
            Ok((artifact, path)) => {
                for issue in Self::validate_constitution(&artifact) {
                    push_issue(GovernanceDiagnosticIssue {
                        code: issue.code,
                        severity: "error".to_string(),
                        message: issue.message,
                        hint: Some("Run 'hivemind constitution validate <project>' and fix the invalid rule or partition".to_string()),
                        artifact_kind: Some("constitution".to_string()),
                        artifact_id: Some("constitution.yaml".to_string()),
                        template_id: None,
                        path: Some(path.to_string_lossy().to_string()),
                    });
                }
            }
            Err(err) => {
                push_issue(GovernanceDiagnosticIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    hint: err.recovery_hint,
                    artifact_kind: Some("constitution".to_string()),
                    artifact_id: Some("constitution.yaml".to_string()),
                    template_id: None,
                    path: err
                        .context
                        .get("path")
                        .cloned()
                        .or_else(|| Some(constitution_path.to_string_lossy().to_string())),
                });
            }
        }

        if !project.repositories.is_empty() {
            let snapshot_path = self
                .governance_graph_snapshot_location(project.id)
                .path
                .to_string_lossy()
                .to_string();
            if let Err(err) = self.ensure_graph_snapshot_current_for_constitution(&project, origin)
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    hint: err.recovery_hint,
                    artifact_kind: Some("graph_snapshot".to_string()),
                    artifact_id: Some("graph_snapshot.json".to_string()),
                    template_id: None,
                    path: Some(snapshot_path),
                });
            }
        }

        let template_root = self.governance_global_root().join("templates");
        for path in Self::governance_json_paths(&template_root, origin)? {
            let template_id = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map_or_else(|| "unknown".to_string(), std::string::ToString::to_string);
            let path_rendered = path.to_string_lossy().to_string();

            let artifact = match Self::read_governance_json::<GlobalTemplateArtifact>(
                &path,
                "global_template",
                &template_id,
                origin,
            ) {
                Ok(artifact) => artifact,
                Err(err) => {
                    push_issue(GovernanceDiagnosticIssue {
                        code: err.code,
                        severity: "error".to_string(),
                        message: err.message,
                        hint: err.recovery_hint,
                        artifact_kind: Some("template".to_string()),
                        artifact_id: Some(template_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered),
                    });
                    continue;
                }
            };

            if artifact.template_id != template_id {
                push_issue(GovernanceDiagnosticIssue {
                    code: "template_id_mismatch".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Template file key mismatch: expected '{template_id}', found '{}'",
                        artifact.template_id
                    ),
                    hint: Some("Rename the template file or fix template_id in JSON".to_string()),
                    artifact_kind: Some("template".to_string()),
                    artifact_id: Some(template_id.clone()),
                    template_id: Some(template_id.clone()),
                    path: Some(path_rendered.clone()),
                });
            }

            if self
                .read_global_system_prompt_artifact(&artifact.system_prompt_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "template_system_prompt_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Template '{template_id}' references missing system prompt '{}'",
                        artifact.system_prompt_id
                    ),
                    hint: Some(
                        "Create the missing system prompt or update template.system_prompt_id"
                            .to_string(),
                    ),
                    artifact_kind: Some("system_prompt".to_string()),
                    artifact_id: Some(artifact.system_prompt_id.clone()),
                    template_id: Some(template_id.clone()),
                    path: Some(path_rendered.clone()),
                });
            }

            for skill_id in &artifact.skill_ids {
                if self.read_global_skill_artifact(skill_id, origin).is_err() {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "template_skill_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Template '{template_id}' references missing global skill '{skill_id}'"
                        ),
                        hint: Some(
                            "Create the missing skill or remove it from template.skill_ids"
                                .to_string(),
                        ),
                        artifact_kind: Some("skill".to_string()),
                        artifact_id: Some(skill_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered.clone()),
                    });
                }
            }

            for document_id in &artifact.document_ids {
                if self
                    .read_project_document_artifact(project.id, document_id, origin)
                    .is_err()
                {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "template_document_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Template '{template_id}' references missing project document '{document_id}'"
                        ),
                        hint: Some("Create the project document or remove it from template.document_ids".to_string()),
                        artifact_kind: Some("document".to_string()),
                        artifact_id: Some(document_id.clone()),
                        template_id: Some(template_id.clone()),
                        path: Some(path_rendered.clone()),
                    });
                }
            }
        }

        if let Some(snapshot) = self.latest_template_instantiation_snapshot(project.id)? {
            if self
                .read_global_template_artifact(&snapshot.template_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "instantiated_template_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Latest instantiated template '{}' no longer exists",
                        snapshot.template_id
                    ),
                    hint: Some(
                        "Recreate the template or instantiate a new valid template for this project"
                            .to_string(),
                    ),
                    artifact_kind: Some("template".to_string()),
                    artifact_id: Some(snapshot.template_id.clone()),
                    template_id: Some(snapshot.template_id.clone()),
                    path: Some(
                        self.global_template_path(&snapshot.template_id)
                            .to_string_lossy()
                            .to_string(),
                    ),
                });
            }

            if self
                .read_global_system_prompt_artifact(&snapshot.system_prompt_id, origin)
                .is_err()
            {
                push_issue(GovernanceDiagnosticIssue {
                    code: "instantiated_system_prompt_missing".to_string(),
                    severity: "error".to_string(),
                    message: format!(
                        "Latest instantiated template '{}' references missing system prompt '{}'",
                        snapshot.template_id, snapshot.system_prompt_id
                    ),
                    hint: Some(
                        "Recreate the system prompt and re-instantiate the template".to_string(),
                    ),
                    artifact_kind: Some("system_prompt".to_string()),
                    artifact_id: Some(snapshot.system_prompt_id.clone()),
                    template_id: Some(snapshot.template_id.clone()),
                    path: None,
                });
            }

            for skill_id in &snapshot.skill_ids {
                if self.read_global_skill_artifact(skill_id, origin).is_err() {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "instantiated_skill_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Latest instantiated template '{}' references missing skill '{}'",
                            snapshot.template_id, skill_id
                        ),
                        hint: Some(
                            "Recreate the skill and re-instantiate the template".to_string(),
                        ),
                        artifact_kind: Some("skill".to_string()),
                        artifact_id: Some(skill_id.clone()),
                        template_id: Some(snapshot.template_id.clone()),
                        path: None,
                    });
                }
            }

            for document_id in &snapshot.document_ids {
                if self
                    .read_project_document_artifact(project.id, document_id, origin)
                    .is_err()
                {
                    push_issue(GovernanceDiagnosticIssue {
                        code: "instantiated_document_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Latest instantiated template '{}' references missing project document '{}'",
                            snapshot.template_id, document_id
                        ),
                        hint: Some("Create the document and re-instantiate the template".to_string()),
                        artifact_kind: Some("document".to_string()),
                        artifact_id: Some(document_id.clone()),
                        template_id: Some(snapshot.template_id.clone()),
                        path: None,
                    });
                }
            }
        }

        let repair_probe = self.build_governance_repair_plan(&project, None, false)?;
        for issue in repair_probe.result.issues {
            push_issue(GovernanceDiagnosticIssue {
                code: issue.code,
                severity: issue.severity,
                message: issue.message,
                hint: issue.hint,
                artifact_kind: issue.artifact_kind,
                artifact_id: issue.artifact_id,
                template_id: None,
                path: issue.path,
            });
        }

        let mut seen = HashSet::new();
        issues.retain(|issue| {
            let key = format!(
                "{}|{}|{}|{}|{}",
                issue.code,
                issue.artifact_id.clone().unwrap_or_default(),
                issue.template_id.clone().unwrap_or_default(),
                issue.path.clone().unwrap_or_default(),
                issue.message
            );
            seen.insert(key)
        });
        issues.sort_by(|a, b| {
            a.code
                .cmp(&b.code)
                .then(a.template_id.cmp(&b.template_id))
                .then(a.artifact_id.cmp(&b.artifact_id))
                .then(a.message.cmp(&b.message))
        });

        let issue_count = issues.len();
        Ok(ProjectGovernanceDiagnosticsResult {
            project_id: project.id,
            checked_at: Utc::now(),
            healthy: issue_count == 0,
            issue_count,
            issues,
        })
    }
}
