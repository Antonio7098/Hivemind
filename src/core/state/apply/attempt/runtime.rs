use super::*;

impl AppState {
    pub(super) fn apply_attempt_runtime_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
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
