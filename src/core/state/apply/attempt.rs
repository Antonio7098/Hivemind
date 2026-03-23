use super::*;

mod checkpoints;
mod checks;
mod lifecycle;
mod runtime;

impl AppState {
    pub(super) fn apply_attempt_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        self.apply_attempt_lifecycle_event(payload, timestamp)
            || self.apply_attempt_checkpoint_event(payload, timestamp)
            || self.apply_attempt_checks_event(payload, timestamp)
            || self.apply_attempt_runtime_event(payload, timestamp)
    }
}
