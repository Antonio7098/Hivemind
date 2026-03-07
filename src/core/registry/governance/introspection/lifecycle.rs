use super::*;

impl Registry {
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
}
