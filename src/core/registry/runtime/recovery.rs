use super::*;

impl Registry {
    pub(crate) fn select_runtime_backup(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        primary: &ProjectRuntimeConfig,
    ) -> Option<ProjectRuntimeConfig> {
        if let Ok(candidate) = Self::effective_runtime_for_task(
            state,
            flow,
            task_id,
            RuntimeRole::Validator,
            "registry:tick_flow",
        ) {
            if candidate != *primary {
                return Some(candidate);
            }
        }

        match primary.adapter_name.as_str() {
            "opencode" => {
                let mut fallback = primary.clone();
                fallback.adapter_name = "kilo".to_string();
                Some(fallback)
            }
            "kilo" => {
                let mut fallback = primary.clone();
                fallback.adapter_name = "opencode".to_string();
                Some(fallback)
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn schedule_runtime_recovery(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        from_runtime: &ProjectRuntimeConfig,
        classified: &ClassifiedRuntimeError,
        origin: &'static str,
    ) -> Result<()> {
        let backup = Self::select_runtime_backup(state, flow, task_id, from_runtime);
        let target_runtime = if classified.rate_limited {
            backup.unwrap_or_else(|| from_runtime.clone())
        } else {
            from_runtime.clone()
        };

        let strategy = if target_runtime.adapter_name == from_runtime.adapter_name {
            "same_runtime_retry"
        } else {
            "fallback_runtime"
        };
        let backoff_ms = if classified.rate_limited { 1_000 } else { 250 };

        let corr_attempt =
            Self::correlation_for_flow_task_attempt_event(state, flow, task_id, attempt_id);
        self.append_event(
            Event::new(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: from_runtime.adapter_name.clone(),
                    to_adapter: target_runtime.adapter_name.clone(),
                    strategy: strategy.to_string(),
                    reason: classified.code.clone(),
                    backoff_ms,
                },
                corr_attempt,
            ),
            origin,
        )?;

        if target_runtime.adapter_name != from_runtime.adapter_name {
            let corr_task = Self::correlation_for_flow_task_event(state, flow, task_id);
            self.append_event(
                Event::new(
                    EventPayload::TaskRuntimeRoleConfigured {
                        task_id,
                        role: RuntimeRole::Worker,
                        adapter_name: target_runtime.adapter_name,
                        binary_path: target_runtime.binary_path,
                        model: target_runtime.model,
                        args: target_runtime.args,
                        env: target_runtime.env,
                        timeout_ms: target_runtime.timeout_ms,
                    },
                    corr_task,
                ),
                origin,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_runtime_failure(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime: &ProjectRuntimeConfig,
        next_attempt_number: u32,
        max_attempts: u32,
        failure_code: &str,
        failure_message: &str,
        recoverable: bool,
        stdout: &str,
        stderr: &str,
        origin: &'static str,
    ) -> Result<()> {
        let corr_attempt =
            Self::correlation_for_flow_task_attempt_event(state, flow, task_id, attempt_id);

        let reason = format!("{failure_code}: {failure_message}");
        self.append_event(
            Event::new(
                EventPayload::RuntimeTerminated { attempt_id, reason },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let classified = Self::classify_runtime_error(
            failure_code,
            failure_message,
            recoverable,
            stdout,
            stderr,
        );
        self.emit_runtime_error_classified(
            flow,
            task_id,
            attempt_id,
            &runtime.adapter_name,
            &classified,
            origin,
        )?;

        if failure_code == "checkpoints_incomplete" {
            return Ok(());
        }

        self.fail_running_attempt(flow, task_id, attempt_id, failure_code, origin)?;

        let should_retry = classified.retryable
            && next_attempt_number < max_attempts
            && (classified.rate_limited
                || matches!(
                    classified.code.as_str(),
                    "timeout" | "wait_failed" | "stdin_write_failed"
                )
                || classified.code.starts_with("native_transport_")
                || classified.code.starts_with("native_stream_"));

        if should_retry {
            self.schedule_runtime_recovery(
                state,
                flow,
                task_id,
                attempt_id,
                runtime,
                &classified,
                origin,
            )?;
            if let Err(err) = self.retry_task(&task_id.to_string(), false, RetryMode::Continue) {
                self.record_error_event(&err, corr_attempt);
            }
        }

        Ok(())
    }
}
