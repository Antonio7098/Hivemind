use super::*;

impl Registry {
    // ARCH_DEBT: legacy function with too many arguments awaiting refactor
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_runtime_output_stream(
        &self,
        attempt_id: Uuid,
        attempt_corr: &CorrelationIds,
        runtime_projector: &mut RuntimeEventProjector,
        stream: RuntimeOutputStream,
        output: &str,
        has_structured_command_events: bool,
        origin: &'static str,
    ) -> Result<()> {
        for chunk in output.lines() {
            let content = chunk.to_string();
            let event = Event::new(
                EventPayload::RuntimeOutputChunk {
                    attempt_id,
                    stream,
                    content: content.clone(),
                },
                attempt_corr.clone(),
            );
            self.store
                .append(event)
                .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

            let observations = runtime_projector.observe_chunk(stream, &format!("{content}\n"));
            let _ = self.append_projected_runtime_observations(
                attempt_id,
                attempt_corr,
                filter_projected_runtime_observations(observations, has_structured_command_events),
                origin,
            );
        }
        Ok(())
    }
}
