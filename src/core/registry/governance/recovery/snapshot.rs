use super::*;

impl Registry {
    pub(crate) fn governance_snapshot_root(&self, project_id: Uuid) -> PathBuf {
        self.governance_project_root(project_id)
            .join("recovery")
            .join("snapshots")
    }

    pub(crate) fn governance_snapshot_path(&self, project_id: Uuid, snapshot_id: &str) -> PathBuf {
        self.governance_snapshot_root(project_id)
            .join(format!("{snapshot_id}.json"))
    }

    pub(crate) fn load_snapshot_manifest(
        &self,
        project_id: Uuid,
        snapshot_id: &str,
        origin: &'static str,
    ) -> Result<GovernanceRecoverySnapshotManifest> {
        let path = self.governance_snapshot_path(project_id, snapshot_id);
        if !path.is_file() {
            return Err(HivemindError::user(
                "governance_snapshot_not_found",
                format!("Governance snapshot '{snapshot_id}' not found"),
                origin,
            )
            .with_hint(
                "Use 'hivemind project governance snapshot list <project>' to inspect available snapshots",
            ));
        }
        let raw = fs::read_to_string(&path).map_err(|e| {
            HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        serde_json::from_str::<GovernanceRecoverySnapshotManifest>(&raw).map_err(|e| {
            HivemindError::user(
                "governance_snapshot_schema_invalid",
                format!("Malformed governance snapshot '{snapshot_id}': {e}"),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string())
            .with_hint("Delete the invalid snapshot and create a new one")
        })
    }

    pub(crate) fn list_snapshot_manifests(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<(PathBuf, GovernanceRecoverySnapshotManifest)>> {
        let root = self.governance_snapshot_root(project_id);
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| {
            HivemindError::system("governance_snapshot_list_failed", e.to_string(), origin)
                .with_context("path", root.to_string_lossy().to_string())
        })? {
            let entry = entry.map_err(|e| {
                HivemindError::system("governance_snapshot_list_failed", e.to_string(), origin)
            })?;
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path).map_err(|e| {
                HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                    .with_context("path", path.to_string_lossy().to_string())
            })?;
            let manifest = serde_json::from_str::<GovernanceRecoverySnapshotManifest>(&raw)
                .map_err(|e| {
                    HivemindError::user(
                        "governance_snapshot_schema_invalid",
                        format!("Malformed governance snapshot at '{}': {e}", path.display()),
                        origin,
                    )
                    .with_context("path", path.to_string_lossy().to_string())
                })?;
            if manifest.project_id == project_id {
                items.push((path, manifest));
            }
        }
        items.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));
        Ok(items)
    }

    pub(crate) fn governance_snapshot_summary(
        path: &Path,
        manifest: &GovernanceRecoverySnapshotManifest,
    ) -> GovernanceSnapshotSummary {
        GovernanceSnapshotSummary {
            snapshot_id: manifest.snapshot_id.clone(),
            path: path.to_string_lossy().to_string(),
            created_at: manifest.created_at,
            artifact_count: manifest.artifact_count,
            total_bytes: manifest.total_bytes,
            source_event_sequence: manifest.source_event_sequence,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn project_governance_snapshot_create(
        &self,
        id_or_name: &str,
        interval_minutes: Option<u64>,
    ) -> Result<ProjectGovernanceSnapshotCreateResult> {
        let origin = "registry:project_governance_snapshot_create";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        self.ensure_governance_layout(project.id, origin)?;

        if let Some(minutes) = interval_minutes {
            let manifests = self.list_snapshot_manifests(project.id, origin)?;
            if let Some((path, latest)) = manifests.first() {
                let window_secs = minutes.saturating_mul(60);
                let age_secs = Utc::now()
                    .signed_duration_since(latest.created_at)
                    .num_seconds()
                    .max(0)
                    .cast_unsigned();
                if age_secs <= window_secs {
                    return Ok(ProjectGovernanceSnapshotCreateResult {
                        project_id: project.id,
                        reused_existing: true,
                        interval_minutes: Some(minutes),
                        snapshot: Self::governance_snapshot_summary(path, latest),
                    });
                }
            }
        }

        let state = self.state()?;
        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let locations = self.governance_managed_file_locations(project.id, origin)?;

        let mut artifacts = Vec::new();
        let mut total_bytes = 0u64;
        for location in locations {
            if !location.path.is_file() {
                continue;
            }
            let content = fs::read_to_string(&location.path).map_err(|e| {
                HivemindError::system("governance_snapshot_read_failed", e.to_string(), origin)
                    .with_context("path", location.path.to_string_lossy().to_string())
            })?;
            total_bytes = total_bytes.saturating_add(content.len() as u64);
            let projection_key = Self::governance_projection_key_from_parts(
                location.project_id,
                location.scope,
                location.artifact_kind,
                &location.artifact_key,
            );
            let revision = projection_map
                .get(&projection_key)
                .map_or(0, |artifact| artifact.revision);
            artifacts.push(GovernanceRecoverySnapshotEntry {
                path: location.path.to_string_lossy().to_string(),
                scope: location.scope.to_string(),
                artifact_kind: location.artifact_kind.to_string(),
                artifact_key: location.artifact_key,
                project_id: location.project_id,
                revision,
                content_hash: Self::constitution_digest(content.as_bytes()),
                content,
            });
        }
        artifacts.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
                .then(a.path.cmp(&b.path))
        });

        let source_event_sequence = self
            .store
            .read_all()
            .ok()
            .and_then(|events| events.last().and_then(|event| event.metadata.sequence));
        let snapshot_id = Uuid::new_v4().to_string();
        let snapshot_dir = self.governance_snapshot_root(project.id);
        fs::create_dir_all(&snapshot_dir).map_err(|e| {
            HivemindError::system("governance_snapshot_write_failed", e.to_string(), origin)
        })?;
        let path = self.governance_snapshot_path(project.id, &snapshot_id);
        let manifest = GovernanceRecoverySnapshotManifest {
            schema_version: GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION.to_string(),
            snapshot_id: snapshot_id.clone(),
            project_id: project.id,
            created_at: Utc::now(),
            source_event_sequence,
            artifact_count: artifacts.len(),
            total_bytes,
            artifacts,
        };
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| {
            HivemindError::system(
                "governance_snapshot_serialize_failed",
                e.to_string(),
                origin,
            )
        })?;
        fs::write(&path, bytes).map_err(|e| {
            HivemindError::system("governance_snapshot_write_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;

        self.append_event(
            Event::new(
                EventPayload::GovernanceSnapshotCreated {
                    project_id: project.id,
                    snapshot_id,
                    path: path.to_string_lossy().to_string(),
                    artifact_count: manifest.artifact_count,
                    total_bytes: manifest.total_bytes,
                    source_event_sequence,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceSnapshotCreateResult {
            project_id: project.id,
            reused_existing: false,
            interval_minutes,
            snapshot: Self::governance_snapshot_summary(&path, &manifest),
        })
    }

    pub fn project_governance_snapshot_list(
        &self,
        id_or_name: &str,
        limit: usize,
    ) -> Result<ProjectGovernanceSnapshotListResult> {
        let origin = "registry:project_governance_snapshot_list";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let mut snapshots: Vec<GovernanceSnapshotSummary> = self
            .list_snapshot_manifests(project.id, origin)?
            .into_iter()
            .map(|(path, manifest)| Self::governance_snapshot_summary(&path, &manifest))
            .collect();
        if limit > 0 && snapshots.len() > limit {
            snapshots.truncate(limit);
        }
        Ok(ProjectGovernanceSnapshotListResult {
            project_id: project.id,
            snapshot_count: snapshots.len(),
            snapshots,
        })
    }
}
