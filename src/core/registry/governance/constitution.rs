use super::*;

impl Registry {
    pub(crate) fn default_constitution_artifact() -> ConstitutionArtifact {
        ConstitutionArtifact {
            version: CONSTITUTION_VERSION,
            schema_version: CONSTITUTION_SCHEMA_VERSION.to_string(),
            compatibility: ConstitutionCompatibility {
                minimum_hivemind_version: env!("CARGO_PKG_VERSION").to_string(),
                governance_schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            },
            partitions: Vec::new(),
            rules: Vec::new(),
        }
    }

    pub(crate) fn constitution_path(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("constitution.yaml")
    }

    pub(crate) fn governance_constitution_location(
        &self,
        project_id: Uuid,
    ) -> GovernanceArtifactLocation {
        GovernanceArtifactLocation {
            project_id: Some(project_id),
            scope: "project",
            artifact_kind: "constitution",
            artifact_key: "constitution.yaml".to_string(),
            is_dir: false,
            path: self.constitution_path(project_id),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn validate_constitution(
        artifact: &ConstitutionArtifact,
    ) -> Vec<ConstitutionValidationIssue> {
        let mut issues = Vec::new();

        if artifact.version != CONSTITUTION_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_version_unsupported".to_string(),
                field: "version".to_string(),
                message: format!(
                    "Unsupported constitution version {}; expected {CONSTITUTION_VERSION}",
                    artifact.version
                ),
            });
        }
        if artifact.schema_version.trim() != CONSTITUTION_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_schema_version_unsupported".to_string(),
                field: "schema_version".to_string(),
                message: format!(
                    "Unsupported schema_version '{}'; expected '{CONSTITUTION_SCHEMA_VERSION}'",
                    artifact.schema_version
                ),
            });
        }
        if artifact
            .compatibility
            .minimum_hivemind_version
            .trim()
            .is_empty()
        {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_minimum_hivemind_version_missing".to_string(),
                field: "compatibility.minimum_hivemind_version".to_string(),
                message: "minimum_hivemind_version cannot be empty".to_string(),
            });
        }
        if artifact.compatibility.governance_schema_version.trim() != GOVERNANCE_SCHEMA_VERSION {
            issues.push(ConstitutionValidationIssue {
                code: "constitution_governance_schema_mismatch".to_string(),
                field: "compatibility.governance_schema_version".to_string(),
                message: format!("governance_schema_version must be '{GOVERNANCE_SCHEMA_VERSION}'"),
            });
        }

        let mut partition_ids = HashSet::new();
        let mut partition_paths = HashSet::new();
        for partition in &artifact.partitions {
            let partition_id = partition.id.trim();
            if partition_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_missing".to_string(),
                    field: "partitions[].id".to_string(),
                    message: "Partition id cannot be empty".to_string(),
                });
            } else if !partition_ids.insert(partition_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_id_duplicate".to_string(),
                    field: format!("partitions.{}", partition.id),
                    message: format!("Duplicate partition id '{}'", partition.id),
                });
            }

            let partition_path = partition.path.trim();
            if partition_path.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_missing".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: "Partition path cannot be empty".to_string(),
                });
            } else if !partition_paths.insert(partition_path.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_partition_path_duplicate".to_string(),
                    field: format!("partitions.{}.path", partition.id),
                    message: format!("Duplicate partition path '{}'", partition.path),
                });
            }
        }

        let mut rule_ids = HashSet::new();
        for rule in &artifact.rules {
            let rule_id = rule.id().trim();
            if rule_id.is_empty() {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_missing".to_string(),
                    field: "rules[].id".to_string(),
                    message: "Rule id cannot be empty".to_string(),
                });
                continue;
            }
            if !rule_ids.insert(rule_id.to_string()) {
                issues.push(ConstitutionValidationIssue {
                    code: "constitution_rule_id_duplicate".to_string(),
                    field: format!("rules.{rule_id}"),
                    message: format!("Duplicate rule id '{rule_id}'"),
                });
            }

            match rule {
                ConstitutionRule::ForbiddenDependency { from, to, .. }
                | ConstitutionRule::AllowedDependency { from, to, .. } => {
                    if from.trim() == to.trim() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_self_dependency".to_string(),
                            field: format!("rules.{rule_id}"),
                            message: "Rule cannot reference the same partition for from/to"
                                .to_string(),
                        });
                    }
                    for (field, partition_ref) in [("from", from), ("to", to)] {
                        if partition_ref.trim().is_empty() {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_missing".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!("Rule field '{field}' cannot be empty"),
                            });
                        } else if !partition_ids.contains(partition_ref.trim()) {
                            issues.push(ConstitutionValidationIssue {
                                code: "constitution_rule_partition_unknown".to_string(),
                                field: format!("rules.{rule_id}.{field}"),
                                message: format!(
                                    "Rule references unknown partition '{partition_ref}'"
                                ),
                            });
                        }
                    }
                }
                ConstitutionRule::CoverageRequirement {
                    target, threshold, ..
                } => {
                    if target.trim().is_empty() {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_missing".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: "Coverage target partition cannot be empty".to_string(),
                        });
                    } else if !partition_ids.contains(target.trim()) {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_target_unknown".to_string(),
                            field: format!("rules.{rule_id}.target"),
                            message: format!(
                                "Coverage rule references unknown partition '{target}'"
                            ),
                        });
                    }
                    if *threshold > 100 {
                        issues.push(ConstitutionValidationIssue {
                            code: "constitution_rule_threshold_invalid".to_string(),
                            field: format!("rules.{rule_id}.threshold"),
                            message: "Coverage threshold must be between 0 and 100".to_string(),
                        });
                    }
                }
            }
        }

        issues
    }

    pub(crate) fn read_constitution_artifact(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<(ConstitutionArtifact, PathBuf)> {
        let path = self.constitution_path(project_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "constitution_not_found",
                "Project constitution is missing",
                origin,
            )
            .with_hint("Run 'hivemind constitution init <project> --confirm' to initialize it"));
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let artifact = Self::parse_constitution_yaml(&raw, origin)
            .map_err(|err| err.with_context("path", path.to_string_lossy().to_string()))?;
        Ok((artifact, path))
    }

    pub(crate) fn parse_constitution_yaml(
        raw: &str,
        origin: &'static str,
    ) -> Result<ConstitutionArtifact> {
        serde_yaml::from_str(raw).map_err(|e| {
            HivemindError::user(
                "constitution_schema_invalid",
                format!("Malformed constitution YAML: {e}"),
                origin,
            )
            .with_hint(
                "Fix the constitution YAML schema and rerun 'hivemind constitution validate'",
            )
        })
    }

    pub(crate) fn write_constitution_artifact(
        path: &Path,
        artifact: &ConstitutionArtifact,
        origin: &'static str,
    ) -> Result<String> {
        let mut yaml = serde_yaml::to_string(artifact).map_err(|e| {
            HivemindError::system("constitution_serialize_failed", e.to_string(), origin)
        })?;
        if !yaml.ends_with('\n') {
            yaml.push('\n');
        }
        fs::write(path, yaml.as_bytes()).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        Ok(Self::constitution_digest(yaml.as_bytes()))
    }

    pub(crate) fn constitution_digest(bytes: &[u8]) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    pub(crate) fn resolve_actor(actor: Option<&str>) -> String {
        actor
            .and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .or_else(|| {
                env::var("USER").ok().and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
            })
            .or_else(|| {
                env::var("USERNAME").ok().and_then(|value| {
                    let trimmed = value.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

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

    /// Initializes and validates a project's constitution artifact.
    ///
    /// # Errors
    /// Returns an error if explicit confirmation is missing, project resolution fails,
    /// constitution schema is invalid, or validation checks fail.
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
            let (artifact, _) = self.read_constitution_artifact(project.id, origin)?;
            artifact
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

    /// Shows the current project constitution.
    ///
    /// # Errors
    /// Returns an error if the project or constitution cannot be resolved.
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

    /// Validates project constitution schema and semantics and emits a validation event.
    ///
    /// # Errors
    /// Returns an error if project resolution fails, constitution cannot be loaded,
    /// or event append fails.
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

    /// Updates a project's constitution with explicit confirmation and audit metadata.
    ///
    /// # Errors
    /// Returns an error if confirmation is missing, content is invalid,
    /// constitution is not initialized, or validation fails.
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
