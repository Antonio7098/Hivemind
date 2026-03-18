use super::*;
use crate::app::EventService;
use crate::core::events::RuntimeOutputStream;
use serde_json::json;
use uuid::Uuid;

pub(crate) fn runtime_stream_item(event: Event) -> Option<RuntimeStreamItemView> {
    let sequence = event.metadata.sequence.unwrap_or(0);
    let flow_id = event
        .metadata
        .correlation
        .flow_id
        .map(|value| value.to_string());
    let task_id = event
        .metadata
        .correlation
        .task_id
        .map(|value| value.to_string());
    let attempt_id = event
        .metadata
        .correlation
        .attempt_id
        .map(|value| value.to_string());

    let event_id = event.metadata.id.to_string();
    let timestamp = event.metadata.timestamp;
    let mk = |kind: &str,
              stream: Option<RuntimeOutputStream>,
              title: Option<String>,
              text: Option<String>,
              data: Value| {
        Some(RuntimeStreamItemView {
            event_id: event_id.clone(),
            sequence,
            timestamp,
            flow_id: flow_id.clone(),
            task_id: task_id.clone(),
            attempt_id: attempt_id.clone(),
            kind: kind.to_string(),
            stream: stream.map(|value| format!("{value:?}").to_lowercase()),
            title,
            text,
            data,
        })
    };

    match event.payload {
        EventPayload::RuntimeOutputChunk {
            stream, content, ..
        } => mk(
            "output_chunk",
            Some(stream),
            Some("Runtime output".to_string()),
            Some(content.clone()),
            json!({"content": content}),
        ),
        EventPayload::RuntimeNarrativeOutputObserved {
            stream, content, ..
        } => mk(
            "narrative",
            Some(stream),
            Some("Narrative".to_string()),
            Some(content.clone()),
            json!({"content": content}),
        ),
        EventPayload::RuntimeToolCallObserved {
            stream,
            tool_name,
            details,
            ..
        } => mk(
            "tool_call",
            Some(stream),
            Some(tool_name.clone()),
            Some(details.clone()),
            json!({"tool_name": tool_name, "details": details}),
        ),
        EventPayload::RuntimeTodoSnapshotUpdated { stream, items, .. } => mk(
            "todo_snapshot",
            Some(stream),
            Some("Todo snapshot".to_string()),
            None,
            json!({"items": items}),
        ),
        EventPayload::RuntimeCommandCompleted {
            stream,
            command,
            exit_code,
            output,
            ..
        } => mk(
            "command",
            Some(stream),
            Some(command.clone()),
            output.clone(),
            json!({"command": command, "exit_code": exit_code, "output": output}),
        ),
        EventPayload::RuntimeFilesystemObserved {
            files_created,
            files_modified,
            files_deleted,
            ..
        } => mk(
            "file_change",
            None,
            Some("Filesystem".to_string()),
            None,
            json!({
                "files_created": files_created,
                "files_modified": files_modified,
                "files_deleted": files_deleted,
            }),
        ),
        EventPayload::RuntimeSessionObserved {
            adapter_name,
            stream,
            session_id,
            ..
        } => mk(
            "session",
            Some(stream),
            Some(adapter_name.clone()),
            Some(session_id.clone()),
            json!({"adapter_name": adapter_name, "session_id": session_id}),
        ),
        EventPayload::RuntimeTurnCompleted {
            adapter_name,
            stream,
            ordinal,
            provider_session_id,
            provider_turn_id,
            git_ref,
            commit_sha,
            summary,
            ..
        } => mk(
            "turn",
            Some(stream),
            Some(format!("{adapter_name} turn {ordinal}")),
            summary.clone(),
            json!({
                "adapter_name": adapter_name,
                "ordinal": ordinal,
                "provider_session_id": provider_session_id,
                "provider_turn_id": provider_turn_id,
                "git_ref": git_ref,
                "commit_sha": commit_sha,
                "summary": summary,
            }),
        ),
        EventPayload::RuntimeStarted { adapter_name, .. } => mk(
            "runtime_started",
            None,
            Some(adapter_name.clone()),
            None,
            json!({"adapter_name": adapter_name}),
        ),
        EventPayload::RuntimeExited {
            exit_code,
            duration_ms,
            ..
        } => mk(
            "runtime_exited",
            None,
            Some("Runtime exited".to_string()),
            None,
            json!({"exit_code": exit_code, "duration_ms": duration_ms}),
        ),
        EventPayload::CheckpointDeclared {
            checkpoint_id,
            order,
            total,
            ..
        } => mk(
            "checkpoint_declared",
            None,
            Some(format!("Checkpoint {order}/{total}")),
            None,
            json!({
                "checkpoint_id": checkpoint_id,
                "order": order,
                "total": total,
            }),
        ),
        EventPayload::CheckpointCompleted {
            checkpoint_id,
            commit_hash,
            order,
            summary,
            ..
        } => mk(
            "checkpoint_completed",
            None,
            Some(format!("Checkpoint {order} completed")),
            summary.clone(),
            json!({"checkpoint_id": checkpoint_id, "order": order, "commit_hash": commit_hash, "summary": summary}),
        ),
        EventPayload::CheckpointCommitCreated { commit_sha, .. } => mk(
            "checkpoint_commit_created",
            None,
            Some("Checkpoint commit created".to_string()),
            Some(commit_sha.clone()),
            json!({"commit_sha": commit_sha}),
        ),
        _ => None,
    }
}
