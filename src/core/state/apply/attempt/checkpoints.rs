use super::*;

impl AppState {
    pub(super) fn apply_attempt_checkpoint_event(
        &mut self,
        payload: &EventPayload,
        _timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::CheckpointDeclared {
                attempt_id,
                checkpoint_id,
                order,
                total,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    let exists = attempt
                        .checkpoints
                        .iter()
                        .any(|cp| cp.checkpoint_id == *checkpoint_id);
                    if !exists {
                        attempt.checkpoints.push(AttemptCheckpoint {
                            checkpoint_id: checkpoint_id.clone(),
                            order: *order,
                            total: *total,
                            state: AttemptCheckpointState::Declared,
                            commit_hash: None,
                            completed_at: None,
                            summary: None,
                        });
                        attempt.checkpoints.sort_by_key(|cp| cp.order);
                    }
                }
                true
            }
            EventPayload::CheckpointActivated {
                attempt_id,
                checkpoint_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    for cp in &mut attempt.checkpoints {
                        if cp.checkpoint_id == *checkpoint_id {
                            cp.state = AttemptCheckpointState::Active;
                        } else if cp.state != AttemptCheckpointState::Completed {
                            cp.state = AttemptCheckpointState::Declared;
                        }
                    }
                }
                true
            }
            EventPayload::CheckpointCompleted {
                attempt_id,
                checkpoint_id,
                commit_hash,
                timestamp,
                summary,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    if let Some(cp) = attempt
                        .checkpoints
                        .iter_mut()
                        .find(|cp| cp.checkpoint_id == *checkpoint_id)
                    {
                        cp.state = AttemptCheckpointState::Completed;
                        cp.commit_hash = Some(commit_hash.clone());
                        cp.completed_at = Some(*timestamp);
                        cp.summary.clone_from(summary);
                    }
                }
                true
            }
            EventPayload::AllCheckpointsCompleted { attempt_id, .. } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.all_checkpoints_completed = true;
                }
                true
            }
            _ => false,
        }
    }
}
