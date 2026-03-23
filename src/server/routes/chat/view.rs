use super::*;

pub(super) fn map_session_summary(session: &ChatSessionState) -> ChatSessionSummaryView {
    ChatSessionSummaryView {
        session_id: session.id.to_string(),
        mode: session.mode.clone(),
        title: session.title.clone(),
        project_id: session.project_id.map(|value| value.to_string()),
        task_id: session.task_id.map(|value| value.to_string()),
        flow_id: session.flow_id.map(|value| value.to_string()),
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
        message_count: session.messages.len(),
        last_message_preview: session
            .messages
            .last()
            .map(|message| preview_text(&message.content, 96)),
    }
}

pub(super) fn map_session_inspect(session: &ChatSessionState) -> ChatSessionInspectView {
    ChatSessionInspectView {
        session_id: session.id.to_string(),
        mode: session.mode.clone(),
        title: session.title.clone(),
        project_id: session.project_id.map(|value| value.to_string()),
        task_id: session.task_id.map(|value| value.to_string()),
        flow_id: session.flow_id.map(|value| value.to_string()),
        created_at: session.created_at.to_rfc3339(),
        updated_at: session.updated_at.to_rfc3339(),
        messages: session.messages.iter().map(map_session_message).collect(),
    }
}

pub(super) fn map_session_message(message: &ChatMessageState) -> ChatSessionMessageView {
    ChatSessionMessageView {
        message_id: message.id.to_string(),
        role: message.role.clone(),
        content: message.content.clone(),
        created_at: message.created_at.to_rfc3339(),
        request_id: message.request_id.clone(),
        provider: message.provider.clone(),
        model: message.model.clone(),
        final_state: message.final_state.clone(),
        runtime_selection_source: message.runtime_selection_source.clone(),
    }
}

pub(super) fn preview_text(content: &str, max_chars: usize) -> String {
    let trimmed = content.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let preview = trimmed.chars().take(max_chars).collect::<String>();
    format!("{preview}…")
}
