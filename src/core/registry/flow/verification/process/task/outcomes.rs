use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_scope_violation(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        verification: &VerificationResult,
        task: &crate::core::graph::GraphTask,
        worktree_status: &WorktreeStatus,
        origin: &'static str,
    ) -> Result<()> {
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);

        if let Some(scope) = &task.scope {
            self.append_event(
                Event::new(
                    EventPayload::ScopeViolationDetected {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        verification_id: verification.id,
                        verified_at: verification.verified_at,
                        scope: scope.clone(),
                        violations: verification.violations.clone(),
                    },
                    CorrelationIds::for_graph_flow_task_attempt(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        task_id,
                        attempt_id,
                    ),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    from: TaskExecState::Verifying,
                    to: TaskExecState::Failed,
                },
                corr_task,
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionFailed {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    reason: Some("scope_violation".to_string()),
                },
                CorrelationIds::for_graph_flow_task_attempt(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                    attempt_id,
                ),
            ),
            origin,
        )?;

        let violations = verification
            .violations
            .iter()
            .map(|v| {
                let path = v.path.as_deref().unwrap_or("-");
                format!("{:?}: {path}: {}", v.violation_type, v.description)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Err(HivemindError::scope(
            "scope_violation",
            format!("Scope violation detected:\n{violations}"),
            origin,
        )
        .with_hint(format!(
            "Worktree preserved at {}",
            worktree_status.path.display()
        )))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_verification_failure(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        task: &crate::core::graph::GraphTask,
        exec: &crate::core::flow::TaskExecution,
        results: &[(String, bool, bool)],
        worktree_status: &WorktreeStatus,
        origin: &'static str,
    ) -> Result<()> {
        let max_retries = task.retry_policy.max_retries;
        let max_attempts = max_retries.saturating_add(1);
        let can_retry = exec.attempt_count < max_attempts;
        let to = if can_retry {
            TaskExecState::Retry
        } else {
            TaskExecState::Failed
        };

        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    from: TaskExecState::Verifying,
                    to,
                },
                corr_task,
            ),
            origin,
        )?;

        if matches!(to, TaskExecState::Retry | TaskExecState::Failed) {
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt_id),
                        reason: Some("required_checks_failed".to_string()),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        let failures = results
            .iter()
            .filter(|(_, required, passed)| *required && !*passed)
            .map(|(name, _, _)| name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        let err = HivemindError::verification(
            "required_checks_failed",
            format!("Required checks failed: {failures}"),
            origin,
        )
        .with_hint(format!(
            "View check outputs via `hivemind verify results {}`. Worktree preserved at {}",
            attempt_id,
            worktree_status.path.display()
        ));

        self.record_error_event(&err, corr_attempt);

        Err(err)
    }
}
