use super::*;

impl Registry {
    pub(crate) fn governance_document_attachment_states(
        state: &AppState,
        project_id: Uuid,
        task_id: Uuid,
    ) -> (Vec<String>, Vec<String>) {
        let mut included = Vec::new();
        let mut excluded = Vec::new();
        for item in state.governance_attachments.values().filter(|item| {
            item.project_id == project_id
                && item.task_id == task_id
                && item.artifact_kind == "document"
        }) {
            if item.attached {
                included.push(item.artifact_key.clone());
            } else {
                excluded.push(item.artifact_key.clone());
            }
        }
        included.sort();
        included.dedup();
        excluded.sort();
        excluded.dedup();
        (included, excluded)
    }

    pub(crate) fn governance_artifact_revision(
        state: &AppState,
        project_id: Option<Uuid>,
        scope: &str,
        artifact_kind: &str,
        artifact_key: &str,
    ) -> Option<u64> {
        state
            .governance_artifacts
            .values()
            .find(|item| {
                item.project_id == project_id
                    && item.scope == scope
                    && item.artifact_kind == artifact_kind
                    && item.artifact_key == artifact_key
            })
            .map(|item| item.revision)
    }

    pub(crate) fn latest_template_instantiation_snapshot(
        &self,
        project_id: Uuid,
    ) -> Result<Option<TemplateInstantiationSnapshot>> {
        let events = self.read_events(&EventFilter::for_project(project_id))?;
        let mut snapshot: Option<TemplateInstantiationSnapshot> = None;
        for event in events {
            let event_id = event.id().as_uuid().to_string();
            if let EventPayload::TemplateInstantiated {
                project_id: event_project_id,
                template_id,
                system_prompt_id,
                skill_ids,
                document_ids,
                schema_version,
                projection_version,
            } = event.payload
            {
                if event_project_id != project_id {
                    continue;
                }
                snapshot = Some(TemplateInstantiationSnapshot {
                    event_id,
                    template_id,
                    system_prompt_id,
                    skill_ids,
                    document_ids,
                    schema_version,
                    projection_version,
                });
            }
        }
        Ok(snapshot)
    }

    pub(crate) fn attempt_manifest_hash_for_attempt(
        &self,
        attempt_id: Uuid,
    ) -> Result<Option<String>> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        let mut manifest_hash = None;
        for event in events {
            if let EventPayload::AttemptContextAssembled {
                manifest_hash: hash,
                ..
            } = event.payload
            {
                manifest_hash = Some(hash);
            }
        }
        Ok(manifest_hash)
    }

    /// Initializes canonical governance storage and projections for a project.
    pub fn project_governance_init(&self, id_or_name: &str) -> Result<ProjectGovernanceInitResult> {
        let origin = "registry:project_governance_init";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let created_paths = self.ensure_governance_layout(project.id, origin)?;
        let created_set: HashSet<PathBuf> = created_paths.iter().cloned().collect();
        let corr = CorrelationIds::for_project(project.id);

        if !state.governance_projects.contains_key(&project.id) || !created_paths.is_empty() {
            self.append_event(
                Event::new(
                    EventPayload::GovernanceProjectStorageInitialized {
                        project_id: project.id,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                        root_path: self
                            .governance_project_root(project.id)
                            .to_string_lossy()
                            .to_string(),
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut pending_revisions = HashMap::new();
        for location in self.governance_artifact_locations(project.id) {
            let exists = if location.is_dir {
                location.path.is_dir()
            } else {
                location.path.is_file()
            };
            if !exists {
                continue;
            }

            let projected_exists =
                Self::governance_projection_for_location(&state, &location).is_some();
            let should_emit = created_set.contains(&location.path) || !projected_exists;
            if !should_emit {
                continue;
            }

            let revision =
                Self::next_governance_revision(&state, &location, &mut pending_revisions);
            self.append_event(
                Event::new(
                    EventPayload::GovernanceArtifactUpserted {
                        project_id: location.project_id,
                        scope: location.scope.to_string(),
                        artifact_kind: location.artifact_kind.to_string(),
                        artifact_key: location.artifact_key.clone(),
                        path: location.path.to_string_lossy().to_string(),
                        revision,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut created_paths_rendered: Vec<String> = created_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        created_paths_rendered.sort();

        Ok(ProjectGovernanceInitResult {
            project_id: project.id,
            root_path: self
                .governance_project_root(project.id)
                .to_string_lossy()
                .to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            created_paths: created_paths_rendered,
        })
    }

    /// Migrates legacy governance artifacts from repo-local layout into canonical global governance storage.
    pub fn project_governance_migrate(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceMigrateResult> {
        let origin = "registry:project_governance_migrate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut migrated_paths = BTreeSet::new();
        for mapping in self
            .legacy_governance_mappings(&project)
            .into_iter()
            .filter(|m| m.source.exists())
        {
            let copied = if mapping.destination.is_dir {
                Self::copy_dir_if_missing(&mapping.source, &mapping.destination.path, origin)?
            } else {
                Self::copy_file_if_missing(
                    &mapping.source,
                    &mapping.destination.path,
                    Some(Self::governance_default_file_contents(&mapping.destination)),
                    origin,
                )?
            };

            if copied {
                migrated_paths.insert(mapping.destination.path.to_string_lossy().to_string());
            }
        }

        let _ = self.project_governance_init(&project.id.to_string())?;

        let rollback_hint = format!(
            "Rollback by restoring repo-local governance paths from backups under each attached repo '.hivemind/' directory. New layout root: {}",
            self.governance_project_root(project.id).to_string_lossy()
        );
        let migrated_paths_vec: Vec<String> = migrated_paths.into_iter().collect();

        self.append_event(
            Event::new(
                EventPayload::GovernanceStorageMigrated {
                    project_id: Some(project.id),
                    from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
                    to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
                    migrated_paths: migrated_paths_vec.clone(),
                    rollback_hint: rollback_hint.clone(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceMigrateResult {
            project_id: project.id,
            from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
            to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
            migrated_paths: migrated_paths_vec,
            rollback_hint,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Inspects governance storage and projection status for a project.
    pub fn project_governance_inspect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceInspectResult> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let mut artifacts: Vec<GovernanceArtifactInspect> = self
            .governance_artifact_locations(project.id)
            .into_iter()
            .map(|location| {
                let projection = Self::governance_projection_for_location(&state, &location);
                GovernanceArtifactInspect {
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key,
                    path: location.path.to_string_lossy().to_string(),
                    exists: if location.is_dir {
                        location.path.is_dir()
                    } else {
                        location.path.is_file()
                    },
                    projected: projection.is_some(),
                    revision: projection.map_or(0, |item| item.revision),
                }
            })
            .collect();
        artifacts.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
        });

        let mut migrations: Vec<GovernanceMigrationSummary> = state
            .governance_migrations
            .iter()
            .filter(|migration| {
                migration.project_id.is_none() || migration.project_id == Some(project.id)
            })
            .map(|migration| GovernanceMigrationSummary {
                from_layout: migration.from_layout.clone(),
                to_layout: migration.to_layout.clone(),
                migrated_paths: migration.migrated_paths.clone(),
                rollback_hint: migration.rollback_hint.clone(),
                schema_version: migration.schema_version.clone(),
                projection_version: migration.projection_version,
                migrated_at: migration.migrated_at,
            })
            .collect();
        migrations.sort_by(|a, b| b.migrated_at.cmp(&a.migrated_at));

        let mut legacy_candidates = BTreeSet::new();
        for mapping in self.legacy_governance_mappings(&project) {
            if mapping.source.exists() {
                legacy_candidates.insert(mapping.source.to_string_lossy().to_string());
            }
        }

        let projected_storage = state.governance_projects.get(&project.id);
        Ok(ProjectGovernanceInspectResult {
            project_id: project.id,
            root_path: projected_storage.map_or_else(
                || {
                    self.governance_project_root(project.id)
                        .to_string_lossy()
                        .to_string()
                },
                |item| item.root_path.clone(),
            ),
            initialized: projected_storage.is_some(),
            schema_version: projected_storage.map_or_else(
                || GOVERNANCE_SCHEMA_VERSION.to_string(),
                |item| item.schema_version.clone(),
            ),
            projection_version: projected_storage.map_or(GOVERNANCE_PROJECTION_VERSION, |item| {
                item.projection_version
            }),
            export_import_boundary: GOVERNANCE_EXPORT_IMPORT_BOUNDARY.to_string(),
            worktree_base_dir: WorktreeConfig::default()
                .base_dir
                .to_string_lossy()
                .to_string(),
            artifacts,
            migrations,
            legacy_candidates: legacy_candidates.into_iter().collect(),
        })
    }

    /// Diagnoses governance artifact health for operators.
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
                    message: format!("Latest instantiated template '{}' no longer exists", snapshot.template_id),
                    hint: Some("Recreate the template or instantiate a new valid template for this project".to_string()),
                    artifact_kind: Some("template".to_string()),
                    artifact_id: Some(snapshot.template_id.clone()),
                    template_id: Some(snapshot.template_id.clone()),
                    path: Some(self.global_template_path(&snapshot.template_id).to_string_lossy().to_string()),
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

    pub(crate) fn governance_projection_map_for_project(
        state: &AppState,
        project_id: Uuid,
    ) -> BTreeMap<String, crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .iter()
            .filter(|(_, artifact)| {
                artifact.project_id == Some(project_id) || artifact.project_id.is_none()
            })
            .map(|(key, artifact)| (key.clone(), artifact.clone()))
            .collect()
    }

    /// Replays governance projections for a project from canonical event history.
    pub fn project_governance_replay(
        &self,
        id_or_name: &str,
        verify: bool,
    ) -> Result<ProjectGovernanceReplayResult> {
        let origin = "registry:project_governance_replay";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let all_events = self
            .store
            .read_all()
            .map_err(|e| HivemindError::system("event_read_failed", e.to_string(), origin))?;
        let replayed_once = AppState::replay(&all_events);
        let replayed_twice = AppState::replay(&all_events);
        let current = self.state()?;

        let projection_once =
            Self::governance_projection_map_for_project(&replayed_once, project.id);
        let projection_twice =
            Self::governance_projection_map_for_project(&replayed_twice, project.id);
        let projection_current = Self::governance_projection_map_for_project(&current, project.id);

        let idempotent = projection_once == projection_twice;
        let current_matches_replay = projection_once == projection_current;
        let missing_on_disk = projection_once
            .values()
            .filter(|artifact| !Path::new(&artifact.path).exists())
            .count();

        if verify && (!idempotent || !current_matches_replay || missing_on_disk > 0) {
            let err = HivemindError::system(
                "governance_replay_verification_failed",
                "Governance replay verification failed",
                origin,
            )
            .with_context("missing_on_disk", missing_on_disk.to_string())
            .with_hint("Run 'hivemind project governance repair detect <project>' to inspect projection drift");
            return Err(err);
        }

        let projections = projection_once
            .values()
            .map(|artifact| GovernanceProjectionEntry {
                project_id: artifact.project_id,
                scope: artifact.scope.clone(),
                artifact_kind: artifact.artifact_kind.clone(),
                artifact_key: artifact.artifact_key.clone(),
                path: artifact.path.clone(),
                revision: artifact.revision,
                exists_on_disk: Path::new(&artifact.path).exists(),
            })
            .collect();

        Ok(ProjectGovernanceReplayResult {
            project_id: project.id,
            replayed_at: Utc::now(),
            projection_count: projection_once.len(),
            idempotent,
            current_matches_replay,
            projections,
        })
    }
}
