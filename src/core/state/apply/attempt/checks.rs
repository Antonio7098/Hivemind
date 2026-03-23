use super::*;

impl AppState {
    pub(super) fn apply_attempt_checks_event(
        &mut self,
        payload: &EventPayload,
        _timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::CheckCompleted {
                attempt_id,
                check_name,
                passed,
                exit_code,
                output,
                duration_ms,
                required,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.check_results.push(CheckResult {
                        name: check_name.clone(),
                        passed: *passed,
                        exit_code: *exit_code,
                        output: output.clone(),
                        duration_ms: *duration_ms,
                        required: *required,
                    });
                }
                true
            }
            _ => false,
        }
    }
}
