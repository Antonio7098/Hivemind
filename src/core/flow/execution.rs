use super::*;

impl TaskExecution {
    #[must_use]
    pub fn new(task_id: Uuid) -> Self {
        Self {
            task_id,
            state: TaskExecState::Pending,
            attempt_count: 0,
            retry_mode: RetryMode::default(),
            frozen_commit_sha: None,
            integrated_commit_sha: None,
            updated_at: Utc::now(),
            blocked_reason: None,
        }
    }

    pub fn transition(&mut self, new_state: TaskExecState) -> Result<(), FlowError> {
        if !self.can_transition_to(new_state) {
            return Err(FlowError::InvalidTransition {
                from: self.state,
                to: new_state,
            });
        }

        self.state = new_state;
        self.updated_at = Utc::now();
        if new_state == TaskExecState::Running {
            self.attempt_count += 1;
        }
        Ok(())
    }

    #[must_use]
    pub fn can_transition_to(&self, new_state: TaskExecState) -> bool {
        use TaskExecState::{
            Escalated, Failed, Pending, Ready, Retry, Running, Success, Verifying,
        };
        matches!(
            (self.state, new_state),
            (Pending, Ready | Running)
                | (Ready | Retry, Running)
                | (Running, Verifying)
                | (Verifying, Success | Retry | Failed)
                | (Failed, Escalated)
        )
    }
}
