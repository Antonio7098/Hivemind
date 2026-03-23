use super::*;

impl Registry {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle_tick_runtime_adapter_error(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime_for_adapter: &ProjectRuntimeConfig,
        next_attempt_number: u32,
        max_attempts: u32,
        error: &RuntimeError,
        origin: &'static str,
    ) -> Result<Option<TickRuntimeExecution>> {
        self.handle_runtime_failure(
            state,
            flow,
            task_id,
            attempt_id,
            runtime_for_adapter,
            next_attempt_number,
            max_attempts,
            &error.code,
            &error.message,
            error.recoverable,
            "",
            "",
            origin,
        )?;
        Ok(None)
    }
}
