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
                .with_hint("Use 'hivemind project governance snapshot list <project>' to inspect available snapshots"));
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

    /// Creates a deterministic governance recovery snapshot for a project.
    ///
    /// # Errors
    /// Returns an error if snapshot persistence fails.
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

    /// Lists governance recovery snapshots for a project.
    ///
    /// # Errors
    /// Returns an error if snapshot listing fails.
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

    /// Restores governance artifacts from a saved snapshot while preserving event authority.
    ///
    /// # Errors
    /// Returns an error if restore confirmation is missing or snapshot files cannot be restored.
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

    #[allow(clippy::too_many_lines)]
    pub(crate) fn build_governance_repair_plan(
        &self,
        project: &Project,
        snapshot: Option<&GovernanceRecoverySnapshotManifest>,
        include_operations: bool,
    ) -> Result<GovernanceRepairPlanBundle> {
        let origin = "registry:build_governance_repair_plan";
        let state = self.state()?;
        let projection_map = Self::governance_projection_map_for_project(&state, project.id);
        let mut issues = Vec::new();
        let mut operations = Vec::new();
        let mut internal = Vec::new();
        let mut issue_seen = HashSet::new();
        let mut op_seen = HashSet::new();

        let snapshot_entries: HashMap<String, GovernanceRecoverySnapshotEntry> = snapshot
            .map(|manifest| {
                manifest
                    .artifacts
                    .iter()
                    .map(|entry| {
                        (
                            Self::governance_projection_key_from_parts(
                                entry.project_id,
                                &entry.scope,
                                &entry.artifact_kind,
                                &entry.artifact_key,
                            ),
                            entry.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut push_issue = |issue: GovernanceDriftIssue| {
            let key = format!(
                "{}|{}|{}|{}",
                issue.code,
                issue.path.clone().unwrap_or_default(),
                issue.artifact_kind.clone().unwrap_or_default(),
                issue.artifact_id.clone().unwrap_or_default()
            );
            if issue_seen.insert(key) {
                issues.push(issue);
            }
        };

        for artifact in projection_map.values() {
            let exists = Path::new(&artifact.path).exists();
            if exists {
                continue;
            }
            let projection_key = Self::governance_projection_key_from_parts(
                artifact.project_id,
                &artifact.scope,
                &artifact.artifact_kind,
                &artifact.artifact_key,
            );
            let recoverable = snapshot_entries
                .get(&projection_key)
                .is_some_and(|entry| entry.revision == artifact.revision);
            let issue = GovernanceDriftIssue {
                code: "governance_artifact_missing".to_string(),
                severity: "error".to_string(),
                message: format!("Governance artifact '{}' is missing on disk", artifact.path),
                recoverable,
                artifact_kind: Some(artifact.artifact_kind.clone()),
                artifact_id: Some(artifact.artifact_key.clone()),
                path: Some(artifact.path.clone()),
                hint: if recoverable {
                    Some("Run 'hivemind project governance repair apply <project> --confirm' to restore from snapshot".to_string())
                } else {
                    Some("Create a new governance snapshot to enable deterministic recovery or recreate the artifact manually".to_string())
                },
            };
            push_issue(issue);

            if include_operations && recoverable {
                let op_key = format!("restore_from_snapshot:{}", artifact.path);
                if op_seen.insert(op_key) {
                    let Some(location) =
                        self.governance_location_for_path(project.id, Path::new(&artifact.path))
                    else {
                        continue;
                    };
                    let Some(entry) = snapshot_entries.get(&projection_key).cloned() else {
                        continue;
                    };
                    operations.push(GovernanceRepairOperation {
                        action: "restore_from_snapshot".to_string(),
                        path: artifact.path.clone(),
                        artifact_kind: Some(artifact.artifact_kind.clone()),
                        artifact_id: Some(artifact.artifact_key.clone()),
                        reason: "Missing artifact file with matching snapshot revision".to_string(),
                    });
                    internal
                        .push(GovernanceRepairInternalOp::RestoreFromSnapshot { location, entry });
                }
            }
        }

        for location in self.governance_managed_file_locations(project.id, origin)? {
            let projection_key = Self::governance_projection_key_from_parts(
                location.project_id,
                location.scope,
                location.artifact_kind,
                &location.artifact_key,
            );
            if !projection_map.contains_key(&projection_key) {
                push_issue(GovernanceDriftIssue {
                        code: "governance_projection_missing".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Governance projection is missing for artifact '{}'",
                            location.path.to_string_lossy()
                        ),
                        recoverable: true,
                        artifact_kind: Some(location.artifact_kind.to_string()),
                        artifact_id: Some(location.artifact_key.clone()),
                        path: Some(location.path.to_string_lossy().to_string()),
                        hint: Some(
                            "Run 'hivemind project governance repair apply <project> --confirm' to rebuild projection metadata".to_string(),
                        ),
                    });
                if include_operations {
                    let op_key =
                        format!("emit_projection_upsert:{}", location.path.to_string_lossy());
                    if op_seen.insert(op_key) {
                        operations.push(GovernanceRepairOperation {
                            action: "emit_projection_upsert".to_string(),
                            path: location.path.to_string_lossy().to_string(),
                            artifact_kind: Some(location.artifact_kind.to_string()),
                            artifact_id: Some(location.artifact_key.clone()),
                            reason:
                                "Artifact exists on disk but lacks governance projection metadata"
                                    .to_string(),
                        });
                        internal.push(GovernanceRepairInternalOp::EmitUpsert {
                            location: location.clone(),
                        });
                    }
                }
            }

            if location.path.extension().is_some_and(|ext| ext == "json") {
                let raw = fs::read_to_string(&location.path).map_err(|e| {
                    HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                        .with_context("path", location.path.to_string_lossy().to_string())
                })?;
                if serde_json::from_str::<serde_json::Value>(&raw).is_err() {
                    let recoverable =
                        projection_map
                            .get(&projection_key)
                            .is_some_and(|projected| {
                                snapshot_entries
                                    .get(&projection_key)
                                    .is_some_and(|entry| entry.revision == projected.revision)
                            });
                    push_issue(GovernanceDriftIssue {
                        code: "governance_artifact_schema_invalid".to_string(),
                        severity: "error".to_string(),
                        message: format!(
                            "Governance artifact '{}' contains malformed JSON",
                            location.path.to_string_lossy()
                        ),
                        recoverable,
                        artifact_kind: Some(location.artifact_kind.to_string()),
                        artifact_id: Some(location.artifact_key.clone()),
                        path: Some(location.path.to_string_lossy().to_string()),
                        hint: if recoverable {
                            Some("Run 'hivemind project governance repair apply <project> --confirm' to restore valid content from snapshot".to_string())
                        } else {
                            Some("Repair the JSON artifact manually or restore from a matching snapshot".to_string())
                        },
                    });
                    if include_operations && recoverable {
                        let op_key =
                            format!("restore_from_snapshot:{}", location.path.to_string_lossy());
                        if op_seen.insert(op_key) {
                            if let Some(entry) = snapshot_entries.get(&projection_key).cloned() {
                                operations.push(GovernanceRepairOperation {
                                    action: "restore_from_snapshot".to_string(),
                                    path: location.path.to_string_lossy().to_string(),
                                    artifact_kind: Some(location.artifact_kind.to_string()),
                                    artifact_id: Some(location.artifact_key.clone()),
                                    reason: "Malformed JSON with matching snapshot revision"
                                        .to_string(),
                                });
                                internal.push(GovernanceRepairInternalOp::RestoreFromSnapshot {
                                    location,
                                    entry,
                                });
                            }
                        }
                    }
                }
            }
        }

        if !project.repositories.is_empty() {
            let graph_snapshot_path = self
                .graph_snapshot_path(project.id)
                .to_string_lossy()
                .to_string();
            if let Err(err) = self.ensure_graph_snapshot_current_for_constitution(project, origin) {
                let graph_issue = matches!(
                    err.code.as_str(),
                    "graph_snapshot_missing"
                        | "graph_snapshot_stale"
                        | "graph_snapshot_integrity_invalid"
                        | "graph_snapshot_profile_mismatch"
                        | "graph_snapshot_schema_invalid"
                );
                push_issue(GovernanceDriftIssue {
                    code: err.code,
                    severity: "error".to_string(),
                    message: err.message,
                    recoverable: graph_issue,
                    artifact_kind: Some("graph_snapshot".to_string()),
                    artifact_id: Some("graph_snapshot.json".to_string()),
                    path: Some(graph_snapshot_path.clone()),
                    hint: err.recovery_hint,
                });
                if include_operations && graph_issue {
                    let op_key = "refresh_graph_snapshot".to_string();
                    if op_seen.insert(op_key) {
                        operations.push(GovernanceRepairOperation {
                            action: "refresh_graph_snapshot".to_string(),
                            path: graph_snapshot_path,
                            artifact_kind: Some("graph_snapshot".to_string()),
                            artifact_id: Some("graph_snapshot.json".to_string()),
                            reason: "Graph snapshot drift is recoverable by deterministic refresh"
                                .to_string(),
                        });
                        internal.push(GovernanceRepairInternalOp::RefreshGraphSnapshot {
                            project_key: project.id.to_string(),
                        });
                    }
                }
            }
        }

        issues.sort_by(|a, b| {
            a.code
                .cmp(&b.code)
                .then(a.path.cmp(&b.path))
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_id.cmp(&b.artifact_id))
        });
        operations.sort_by(|a, b| {
            a.action
                .cmp(&b.action)
                .then(a.path.cmp(&b.path))
                .then(a.artifact_kind.cmp(&b.artifact_kind))
        });

        let issue_count = issues.len();
        let recoverable_issue_count = issues.iter().filter(|item| item.recoverable).count();
        let unrecoverable_issue_count = issue_count.saturating_sub(recoverable_issue_count);
        let result = ProjectGovernanceRepairPlanResult {
            project_id: project.id,
            checked_at: Utc::now(),
            healthy: issue_count == 0,
            issue_count,
            recoverable_issue_count,
            unrecoverable_issue_count,
            snapshot_id: snapshot.map(|item| item.snapshot_id.clone()),
            ready_to_apply: issue_count == 0 || unrecoverable_issue_count == 0,
            issues,
            operations,
        };

        Ok(GovernanceRepairPlanBundle {
            result,
            operations: internal,
        })
    }

    /// Detects governance drift against canonical event history.
    ///
    /// # Errors
    /// Returns an error if project resolution or replay fails.
    pub fn project_governance_repair_detect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        let origin = "registry:project_governance_repair_detect";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let plan = self.build_governance_repair_plan(&project, None, false)?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceDriftDetected {
                    project_id: project.id,
                    issue_count: plan.result.issue_count,
                    recoverable_count: plan.result.recoverable_issue_count,
                    unrecoverable_count: plan.result.unrecoverable_issue_count,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(plan.result)
    }

    /// Previews deterministic governance repair operations.
    ///
    /// # Errors
    /// Returns an error if snapshot loading fails.
    pub fn project_governance_repair_preview(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
    ) -> Result<ProjectGovernanceRepairPlanResult> {
        let origin = "registry:project_governance_repair_preview";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let snapshot = match snapshot_id {
            Some(id) => Some(self.load_snapshot_manifest(project.id, id, origin)?),
            None => None,
        };
        let plan = self.build_governance_repair_plan(&project, snapshot.as_ref(), true)?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceDriftDetected {
                    project_id: project.id,
                    issue_count: plan.result.issue_count,
                    recoverable_count: plan.result.recoverable_issue_count,
                    unrecoverable_count: plan.result.unrecoverable_issue_count,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;
        Ok(plan.result)
    }

    /// Applies deterministic governance repair operations.
    ///
    /// # Errors
    /// Returns an error if confirmation is missing, active flows exist, or repair cannot fully proceed.
    #[allow(clippy::too_many_lines)]
    pub fn project_governance_repair_apply(
        &self,
        id_or_name: &str,
        snapshot_id: Option<&str>,
        confirm: bool,
    ) -> Result<ProjectGovernanceRepairApplyResult> {
        let origin = "registry:project_governance_repair_apply";
        if !confirm {
            return Err(HivemindError::user(
                "repair_confirmation_required",
                "Governance repair apply requires explicit confirmation",
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
                "governance_repair_blocked_active_flow",
                "Cannot apply governance repair while project has active flows",
                origin,
            )
            .with_hint(
                "Complete, merge, or abort active flows before applying governance repair",
            ));
        }

        let snapshot = match snapshot_id {
            Some(id) => Some(self.load_snapshot_manifest(project.id, id, origin)?),
            None => None,
        };
        let plan = self.build_governance_repair_plan(&project, snapshot.as_ref(), true)?;
        if !plan.result.healthy && !plan.result.ready_to_apply {
            return Err(HivemindError::user(
                    "governance_repair_unrecoverable",
                    "Governance repair plan contains unrecoverable issues",
                    origin,
                )
                .with_hint("Run 'hivemind project governance repair preview <project>' and resolve unrecoverable items manually"));
        }

        let mut applied_operations = Vec::new();
        let mut pending = HashMap::new();
        let replay_state = self.state()?;
        for (public_op, internal_op) in plan.result.operations.iter().zip(plan.operations.iter()) {
            match internal_op {
                GovernanceRepairInternalOp::EmitUpsert { location } => {
                    self.append_governance_upsert_for_location(
                        &replay_state,
                        &mut pending,
                        location,
                        CorrelationIds::for_project(project.id),
                        origin,
                    )?;
                }
                GovernanceRepairInternalOp::RestoreFromSnapshot { location, entry } => {
                    if let Some(parent) = Path::new(&entry.path).parent() {
                        fs::create_dir_all(parent).map_err(|e| {
                            HivemindError::system(
                                "governance_repair_apply_failed",
                                e.to_string(),
                                origin,
                            )
                        })?;
                    }
                    fs::write(&entry.path, entry.content.as_bytes()).map_err(|e| {
                        HivemindError::system(
                            "governance_repair_apply_failed",
                            e.to_string(),
                            origin,
                        )
                        .with_context("path", entry.path.clone())
                    })?;
                    self.append_governance_upsert_for_location(
                        &replay_state,
                        &mut pending,
                        location,
                        CorrelationIds::for_project(project.id),
                        origin,
                    )?;
                }
                GovernanceRepairInternalOp::RefreshGraphSnapshot { project_key } => {
                    self.graph_snapshot_refresh(project_key, "governance_repair")?;
                }
            }
            applied_operations.push(public_op.clone());
        }

        let remaining = self.project_governance_repair_detect(&project.id.to_string())?;
        self.append_event(
            Event::new(
                EventPayload::GovernanceRepairApplied {
                    project_id: project.id,
                    operation_count: applied_operations.len(),
                    repaired_count: applied_operations.len(),
                    remaining_issue_count: remaining.issue_count,
                    snapshot_id: snapshot.as_ref().map(|item| item.snapshot_id.clone()),
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceRepairApplyResult {
            project_id: project.id,
            applied_at: Utc::now(),
            snapshot_id: snapshot.map(|item| item.snapshot_id),
            operation_count: applied_operations.len(),
            applied_operations,
            remaining_issue_count: remaining.issue_count,
            remaining_issues: remaining.issues,
        })
    }
}
