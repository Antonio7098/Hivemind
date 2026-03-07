use super::*;

impl Registry {
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
