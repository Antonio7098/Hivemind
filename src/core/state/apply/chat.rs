use super::*;

impl AppState {
    pub(super) fn apply_chat_event(
        &mut self,
        payload: &EventPayload,
        timestamp: DateTime<Utc>,
    ) -> bool {
        match payload {
            EventPayload::ChatSessionCreated {
                session_id,
                mode,
                title,
                project_id,
                task_id,
                flow_id,
            } => {
                self.chat_sessions.insert(
                    *session_id,
                    ChatSessionState {
                        id: *session_id,
                        mode: mode.clone(),
                        title: title.clone(),
                        project_id: *project_id,
                        task_id: *task_id,
                        flow_id: *flow_id,
                        created_at: timestamp,
                        updated_at: timestamp,
                        messages: Vec::new(),
                    },
                );
                true
            }
            EventPayload::ChatSessionTitleUpdated { session_id, title } => {
                if let Some(session) = self.chat_sessions.get_mut(session_id) {
                    session.title.clone_from(title);
                    session.updated_at = timestamp;
                }
                true
            }
            EventPayload::ChatMessageAppended {
                session_id,
                message_id,
                role,
                content,
                request_id,
                provider,
                model,
                final_state,
                runtime_selection_source,
            } => {
                if let Some(session) = self.chat_sessions.get_mut(session_id) {
                    session.messages.push(ChatMessageState {
                        id: *message_id,
                        role: role.clone(),
                        content: content.clone(),
                        created_at: timestamp,
                        request_id: request_id.clone(),
                        provider: provider.clone(),
                        model: model.clone(),
                        final_state: final_state.clone(),
                        runtime_selection_source: runtime_selection_source.clone(),
                    });
                    session.updated_at = timestamp;
                }
                true
            }
            _ => false,
        }
    }
}
