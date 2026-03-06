use super::*;

impl Registry {
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
            push_issue(GovernanceDriftIssue {
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
            });

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
            .with_hint(
                "Run 'hivemind project governance repair preview <project>' and resolve unrecoverable items manually",
            ));
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
