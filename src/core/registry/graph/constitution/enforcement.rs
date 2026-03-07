use super::*;

impl Registry {
    pub(crate) fn append_constitution_violation_events(
        &self,
        project_id: Uuid,
        gate: &str,
        correlation: &CorrelationIds,
        violations: &[ConstitutionRuleViolation],
        origin: &'static str,
    ) -> Result<()> {
        for violation in violations {
            self.append_event(
                Event::new(
                    EventPayload::ConstitutionViolationDetected {
                        project_id,
                        flow_id: correlation.flow_id,
                        task_id: correlation.task_id,
                        attempt_id: correlation.attempt_id,
                        gate: gate.to_string(),
                        rule_id: violation.rule_id.clone(),
                        rule_type: violation.rule_type.clone(),
                        severity: violation.severity.as_str().to_string(),
                        message: violation.message.clone(),
                        evidence: violation.evidence.clone(),
                        remediation_hint: violation.remediation_hint.clone(),
                        blocked: violation.blocked,
                    },
                    correlation.clone(),
                ),
                origin,
            )?;
        }
        Ok(())
    }

    pub(crate) fn run_constitution_check(
        &self,
        project: &Project,
        gate: &str,
        correlation: &CorrelationIds,
        require_initialized: bool,
        origin: &'static str,
    ) -> Result<ProjectConstitutionCheckResult> {
        let state = self.state()?;
        let initialized = state
            .projects
            .get(&project.id)
            .and_then(|item| item.constitution_digest.as_ref())
            .is_some();
        if !initialized {
            if require_initialized {
                return Err(HivemindError::user(
                    "constitution_not_initialized",
                    "Constitution is not initialized for this project",
                    origin,
                )
                .with_hint("Run 'hivemind constitution init <project> --confirm' first"));
            }
            return Ok(ProjectConstitutionCheckResult {
                project_id: project.id,
                gate: gate.to_string(),
                path: self
                    .constitution_path(project.id)
                    .to_string_lossy()
                    .to_string(),
                digest: String::new(),
                schema_version: CONSTITUTION_SCHEMA_VERSION.to_string(),
                constitution_version: CONSTITUTION_VERSION,
                flow_id: correlation.flow_id,
                task_id: correlation.task_id,
                attempt_id: correlation.attempt_id,
                skipped: true,
                skip_reason: Some(
                    "Project constitution is not initialized; enforcement gate skipped".to_string(),
                ),
                violations: Vec::new(),
                hard_violations: 0,
                advisory_violations: 0,
                informational_violations: 0,
                blocked: false,
                checked_at: Utc::now(),
            });
        }

        self.ensure_graph_snapshot_current_for_constitution(project, origin)?;
        let (artifact, path) = self.read_constitution_artifact(project.id, origin)?;
        let raw = fs::read(&path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
        })?;
        let digest = Self::constitution_digest(raw.as_slice());
        let snapshot = self
            .read_graph_snapshot_artifact(project.id, origin)?
            .ok_or_else(|| {
                HivemindError::user(
                    "graph_snapshot_missing",
                    "Graph snapshot is missing for constitution enforcement",
                    origin,
                )
                .with_hint("Run: hivemind graph snapshot refresh <project>")
            })?;
        let violations = Self::evaluate_constitution_rules(&artifact, &snapshot, origin)?;
        if !violations.is_empty() {
            self.append_constitution_violation_events(
                project.id,
                gate,
                correlation,
                &violations,
                origin,
            )?;
        }

        let hard_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Hard))
            .count();
        let advisory_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Advisory))
            .count();
        let informational_violations = violations
            .iter()
            .filter(|item| matches!(item.severity, ConstitutionSeverity::Informational))
            .count();
        Ok(ProjectConstitutionCheckResult {
            project_id: project.id,
            gate: gate.to_string(),
            path: path.to_string_lossy().to_string(),
            digest,
            schema_version: artifact.schema_version,
            constitution_version: artifact.version,
            flow_id: correlation.flow_id,
            task_id: correlation.task_id,
            attempt_id: correlation.attempt_id,
            skipped: false,
            skip_reason: None,
            violations,
            hard_violations,
            advisory_violations,
            informational_violations,
            blocked: hard_violations > 0,
            checked_at: Utc::now(),
        })
    }

    pub(crate) fn enforce_constitution_gate(
        &self,
        project_id: Uuid,
        gate: &'static str,
        correlation: CorrelationIds,
        origin: &'static str,
    ) -> Result<Option<ProjectConstitutionCheckResult>> {
        let project = self
            .get_project(&project_id.to_string())
            .inspect_err(|err| self.record_error_event(err, correlation.clone()))?;
        let result = self.run_constitution_check(&project, gate, &correlation, false, origin)?;
        if result.skipped {
            return Ok(None);
        }
        if result.blocked {
            let hard_rule_ids = result
                .violations
                .iter()
                .filter(|item| matches!(item.severity, ConstitutionSeverity::Hard))
                .map(|item| item.rule_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let err = HivemindError::policy(
                "constitution_hard_violation",
                format!("Constitution hard violations block {gate}: {hard_rule_ids}"),
                origin,
            )
            .with_context("project_id", project_id.to_string())
            .with_context("gate", gate.to_string())
            .with_context("violations", serde_json::to_string(&result.violations).unwrap_or_else(|_| "[]".to_string()))
            .with_hint(format!("Run 'hivemind constitution check --project {project_id}' and remediate blocking rules"));
            self.record_error_event(&err, correlation);
            return Err(err);
        }
        Ok(Some(result))
    }
}
