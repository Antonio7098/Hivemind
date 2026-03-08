#![allow(clippy::too_many_lines)]

use super::*;

impl AppState {
    pub(super) fn apply_attempt_event(
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
            EventPayload::RuntimeSessionObserved {
                attempt_id,
                adapter_name,
                session_id,
                ..
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    attempt.runtime_session = Some(AttemptRuntimeSession {
                        adapter_name: adapter_name.clone(),
                        session_id: session_id.clone(),
                        discovered_at: timestamp,
                    });
                }
                true
            }
            EventPayload::RuntimeTurnCompleted {
                attempt_id,
                ordinal,
                adapter_name,
                stream,
                provider_session_id,
                provider_turn_id,
                git_ref,
                commit_sha,
                summary,
            } => {
                if let Some(attempt) = self.attempts.get_mut(attempt_id) {
                    let exists = attempt
                        .turn_refs
                        .iter()
                        .any(|turn| turn.ordinal == *ordinal);
                    if !exists {
                        attempt.turn_refs.push(AttemptTurnRef {
                            ordinal: *ordinal,
                            adapter_name: adapter_name.clone(),
                            stream: *stream,
                            provider_session_id: provider_session_id.clone(),
                            provider_turn_id: provider_turn_id.clone(),
                            git_ref: git_ref.clone(),
                            commit_sha: commit_sha.clone(),
                            summary: summary.clone(),
                        });
                        attempt.turn_refs.sort_by_key(|turn| turn.ordinal);
                    }
                }
                true
            }
            _ => false,
        }
    }
}
