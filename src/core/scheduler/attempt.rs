use super::*;

impl Attempt {
    /// Creates a new attempt.
    pub fn new(task_id: Uuid, attempt_number: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            attempt_number,
            started_at: chrono::Utc::now(),
            ended_at: None,
            outcome: None,
        }
    }

    /// Completes the attempt with an outcome.
    pub fn complete(&mut self, outcome: AttemptOutcome) {
        self.ended_at = Some(chrono::Utc::now());
        self.outcome = Some(outcome);
    }
}
