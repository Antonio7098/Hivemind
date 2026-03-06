use super::*;

impl Registry {
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
}
