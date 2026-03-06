use super::*;

impl Registry {
    pub fn constitution_check(&self, id_or_name: &str) -> Result<ProjectConstitutionCheckResult> {
        let origin = "registry:constitution_check";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.run_constitution_check(
            &project,
            "manual_check",
            &CorrelationIds::for_project(project.id),
            true,
            origin,
        )
    }

    #[allow(clippy::too_many_lines)]
    pub fn constitution_init(
        &self,
        id_or_name: &str,
        content: Option<&str>,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        let origin = "registry:constitution_init";
        if !confirmed {
            return Err(HivemindError::user(
                "constitution_confirmation_required",
                "Constitution mutation requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        self.ensure_governance_layout(project.id, origin)?;
        let location = self.governance_constitution_location(project.id);
        let artifact = if let Some(raw) = content {
            Self::parse_constitution_yaml(raw, origin)?
        } else if location.path.is_file() {
            self.read_constitution_artifact(project.id, origin)?.0
        } else {
            Self::default_constitution_artifact()
        };
        let issues = Self::validate_constitution(&artifact);
        if !issues.is_empty() {
            return Err(HivemindError::user(
                "constitution_validation_failed",
                format!(
                    "Constitution initialization failed with {} validation issue(s)",
                    issues.len()
                ),
                origin,
            )
            .with_context(
                "issues",
                serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string()),
            )
            .with_hint("Run 'hivemind constitution validate <project>' to inspect the issues"));
        }
        let state = self.state()?;
        let already_initialized = state
            .projects
            .get(&project.id)
            .and_then(|p| p.constitution_digest.as_ref())
            .is_some();
        if already_initialized {
            return Err(HivemindError::user(
                "constitution_already_initialized",
                "Constitution is already initialized for this project",
                origin,
            )
            .with_hint(
                "Use 'hivemind constitution update <project> --confirm ...' for mutations",
            ));
        }
        let digest = Self::write_constitution_artifact(&location.path, &artifact, origin)?;
        let mut pending = HashMap::new();
        let revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;
        let actor_value = Self::resolve_actor(actor);
        let mutation_intent = intent
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "initialize constitution".to_string());
        self.append_event(
            Event::new(
                EventPayload::ConstitutionInitialized {
                    project_id: project.id,
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    digest: digest.clone(),
                    revision,
                    actor: actor_value.clone(),
                    mutation_intent: mutation_intent.clone(),
                    confirmed,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(ProjectConstitutionMutationResult {
            project_id: project.id,
            path: location.path.to_string_lossy().to_string(),
            revision,
            digest,
            previous_digest: None,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            actor: actor_value,
            mutation_intent,
            confirmed,
            updated_at: Utc::now(),
        })
    }

    pub fn constitution_show(&self, id_or_name: &str) -> Result<ProjectConstitutionShowResult> {
        let origin = "registry:constitution_show";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());
        let location = self.governance_constitution_location(project.id);
        let revision = Self::governance_projection_for_location(&state, &location)
            .map_or(0, |item| item.revision);
        Ok(ProjectConstitutionShowResult {
            project_id: project.id,
            path: path.to_string_lossy().to_string(),
            revision,
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            compatibility: artifact.compatibility,
            partitions: artifact.partitions,
            rules: artifact.rules,
        })
    }

    pub fn constitution_validate(
        &self,
        id_or_name: &str,
        validated_by: Option<&str>,
    ) -> Result<ProjectConstitutionValidationResult> {
        let origin = "registry:constitution_validate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let issues = Self::validate_constitution(&artifact);
        let valid = issues.is_empty();
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());
        let validated_by = Self::resolve_actor(validated_by);
        self.append_event(
            Event::new(
                EventPayload::ConstitutionValidated {
                    project_id: project.id,
                    path: path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    digest: digest.clone(),
                    valid,
                    issues: issues
                        .iter()
                        .map(|issue| format!("{}:{}:{}", issue.code, issue.field, issue.message))
                        .collect(),
                    validated_by: validated_by.clone(),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(ProjectConstitutionValidationResult {
            project_id: project.id,
            path: path.to_string_lossy().to_string(),
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            valid,
            issues,
            validated_by,
            validated_at: Utc::now(),
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn constitution_update(
        &self,
        id_or_name: &str,
        content: &str,
        confirmed: bool,
        actor: Option<&str>,
        intent: Option<&str>,
    ) -> Result<ProjectConstitutionMutationResult> {
        let origin = "registry:constitution_update";
        if !confirmed {
            return Err(HivemindError::user(
                "constitution_confirmation_required",
                "Constitution mutation requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
        if content.trim().is_empty() {
            return Err(HivemindError::user(
                "constitution_content_missing",
                "Constitution update requires non-empty YAML content",
                origin,
            )
            .with_hint("Pass --content '<yaml>' or --from-file <path>"));
        }
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_graph_snapshot_current_for_constitution(&project, origin)?;
        self.ensure_governance_layout(project.id, origin)?;
        let state = self.state()?;
        let previous_digest = state
            .projects
            .get(&project.id)
            .and_then(|p| p.constitution_digest.clone())
            .ok_or_else(|| {
                HivemindError::user(
                    "constitution_not_initialized",
                    "Constitution is not initialized for this project",
                    origin,
                )
                .with_hint("Run 'hivemind constitution init <project> --confirm' first")
            })?;
        let location = self.governance_constitution_location(project.id);
        if !location.path.is_file() {
            return Err(HivemindError::user(
                "constitution_not_found",
                "Project constitution is missing",
                origin,
            )
            .with_hint("Run 'hivemind constitution init <project> --confirm' to re-create it"));
        }
        let artifact = Self::parse_constitution_yaml(content, origin)?;
        let issues = Self::validate_constitution(&artifact);
        if !issues.is_empty() {
            return Err(HivemindError::user(
                "constitution_validation_failed",
                format!(
                    "Constitution update failed with {} validation issue(s)",
                    issues.len()
                ),
                origin,
            )
            .with_context(
                "issues",
                serde_json::to_string(&issues).unwrap_or_else(|_| "[]".to_string()),
            )
            .with_hint("Run 'hivemind constitution validate <project>' to inspect the issues"));
        }
        let digest = Self::write_constitution_artifact(&location.path, &artifact, origin)?;
        let mut pending = HashMap::new();
        let revision = self.append_governance_upsert_for_location(
            &state,
            &mut pending,
            &location,
            CorrelationIds::for_project(project.id),
            origin,
        )?;
        let actor_value = Self::resolve_actor(actor);
        let mutation_intent = intent
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "update constitution".to_string());
        self.append_event(
            Event::new(
                EventPayload::ConstitutionUpdated {
                    project_id: project.id,
                    path: location.path.to_string_lossy().to_string(),
                    schema_version: artifact.schema_version.clone(),
                    constitution_version: artifact.version,
                    previous_digest: previous_digest.clone(),
                    digest: digest.clone(),
                    revision,
                    actor: actor_value.clone(),
                    mutation_intent: mutation_intent.clone(),
                    confirmed,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(ProjectConstitutionMutationResult {
            project_id: project.id,
            path: location.path.to_string_lossy().to_string(),
            revision,
            digest,
            previous_digest: Some(previous_digest),
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            actor: actor_value,
            mutation_intent,
            confirmed,
            updated_at: Utc::now(),
        })
    }
}
