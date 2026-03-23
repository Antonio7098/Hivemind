use super::*;

impl Registry {
    pub(super) fn handle_interactive_adapter_event(
        &self,
        attempt_id: Uuid,
        attempt_corr: &CorrelationIds,
        runtime_projector: &mut RuntimeEventProjector,
        stdout: &mut std::io::Stdout,
        event: InteractiveAdapterEvent,
        origin: &'static str,
    ) -> std::result::Result<(), String> {
        match event {
            InteractiveAdapterEvent::Output { content } => {
                let chunk = content;
                let _ = stdout.write_all(chunk.as_bytes());
                let _ = stdout.flush();
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stdout,
                        content: chunk.clone(),
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| e.to_string())?;
                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    attempt_corr,
                    runtime_projector.observe_chunk(RuntimeOutputStream::Stdout, &chunk),
                    origin,
                );
            }
            InteractiveAdapterEvent::Input { content } => {
                let event = Event::new(
                    EventPayload::RuntimeInputProvided {
                        attempt_id,
                        content,
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| e.to_string())?;
            }
            InteractiveAdapterEvent::FilesystemObserved { .. } => {}
            InteractiveAdapterEvent::Interrupted => {
                let event = Event::new(
                    EventPayload::RuntimeInterrupted { attempt_id },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}
