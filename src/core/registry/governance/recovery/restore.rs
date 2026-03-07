use super::*;

impl Registry {
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_snapshot_restore(
        &self,
        id_or_name: &str,
        snapshot_id: &str,
        confirm: bool,
    ) -> Result<ProjectGovernanceSnapshotRestoreResult> {
        let origin = "registry:project_governance_snapshot_restore";
        if !confirm {
            return Err(HivemindError::user(
                "restore_confirmation_required",
                "Snapshot restore requires explicit confirmation",
                origin,
            )
            .with_hint("Re-run with --confirm"));
        }
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;
        let has_active_flow = state.flows.values().any(|flow| {
            flow.project_id == project.id
                && !matches!(
                    flow.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if has_active_flow {
            return Err(HivemindError::user(
                "governance_restore_blocked_active_flow",
                "Cannot restore governance snapshot while project has active flows",
                origin,
            )
            .with_hint(
                "Complete, merge, or abort active flows before restoring governance artifacts",
            ));
        }

        let manifest = self.load_snapshot_manifest(project.id, snapshot_id, origin)?;
        if manifest.schema_version != GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION {
            return Err(HivemindError::user(
                "governance_snapshot_schema_unsupported",
                format!(
                    "Unsupported governance snapshot schema '{}'",
                    manifest.schema_version
                ),
                origin,
            )
            .with_hint("Create a new snapshot with the current Hivemind version"));
        }

        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let mut restored_files = 0usize;
        let mut skipped_files = 0usize;
        let mut stale_files = 0usize;

        for artifact in &manifest.artifacts {
            let projection_key = Self::governance_projection_key_from_parts(
                artifact.project_id,
                &artifact.scope,
                &artifact.artifact_kind,
                &artifact.artifact_key,
            );
            let Some(projected) = projection_map.get(&projection_key) else {
                stale_files += 1;
                skipped_files += 1;
                continue;
            };
            if projected.revision != artifact.revision {
                stale_files += 1;
                skipped_files += 1;
                continue;
            }

            let path = PathBuf::from(&artifact.path);
            let current_same = fs::read_to_string(&path).ok().is_some_and(|raw| {
                Self::constitution_digest(raw.as_bytes()) == artifact.content_hash
            });
            if current_same {
                skipped_files += 1;
                continue;
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "governance_snapshot_restore_failed",
                        e.to_string(),
                        origin,
                    )
                    .with_context("path", path.to_string_lossy().to_string())
                })?;
            }
            fs::write(&path, artifact.content.as_bytes()).map_err(|e| {
                HivemindError::system("governance_snapshot_restore_failed", e.to_string(), origin)
                    .with_context("path", path.to_string_lossy().to_string())
            })?;
            restored_files += 1;
        }

        self.append_event(
            Event::new(
                EventPayload::GovernanceSnapshotRestored {
                    project_id: project.id,
                    snapshot_id: manifest.snapshot_id.clone(),
                    path: self
                        .governance_snapshot_path(project.id, &manifest.snapshot_id)
                        .to_string_lossy()
                        .to_string(),
                    artifact_count: manifest.artifact_count,
                    restored_files,
                    skipped_files,
                    stale_files,
                    repaired_projection_count: 0,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceSnapshotRestoreResult {
            project_id: project.id,
            snapshot_id: manifest.snapshot_id.clone(),
            path: self
                .governance_snapshot_path(project.id, &manifest.snapshot_id)
                .to_string_lossy()
                .to_string(),
            artifact_count: manifest.artifact_count,
            restored_files,
            skipped_files,
            stale_files,
            repaired_projection_count: 0,
        })
    }
}
