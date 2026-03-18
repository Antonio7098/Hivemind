use super::*;

impl AppState {
    pub(super) fn apply_attempt_lifecycle_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::AttemptStarted {
                flow_id,
                task_id,
                attempt_id,
                attempt_number,
            } => {
                self.attempts.insert(
                    *attempt_id,
                    AttemptState {
                        id: *attempt_id,
                        flow_id: *flow_id,
                        task_id: *task_id,
                        attempt_number: *attempt_number,
                        started_at: timestamp,
                        baseline_id: None,
                        diff_id: None,
                        check_results: Vec::new(),
                        checkpoints: Vec::new(),
                        all_checkpoints_completed: false,
                        runtime_session: None,
                        turn_refs: Vec::new(),
                    },
                );
                true
            }
            EventPayload::BaselineCaptured {
                attempt_id,
                baseline_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.baseline_id = Some(*baseline_id);
                }
                true
            }
            EventPayload::DiffComputed {
                attempt_id,
                diff_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.diff_id = Some(*diff_id);
                }
                true
            }
            _ => false,
        }
    }
}
