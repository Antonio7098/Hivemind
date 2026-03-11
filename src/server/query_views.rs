use super::*;
use crate::core::events::RuntimeOutputStream;
use serde_json::json;
use uuid::Uuid;

pub(super) fn list_tasks(registry: &Registry) -> Result<Vec<Task>> {
    let state = registry.state()?;
    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();
    Ok(tasks)
}

pub(super) fn list_graphs(registry: &Registry) -> Result<Vec<TaskGraph>> {
    let state = registry.state()?;
    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();
    Ok(graphs)
}

pub(super) fn list_flows(registry: &Registry) -> Result<Vec<TaskFlow>> {
    let state = registry.state()?;
    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();
    Ok(flows)
}

pub(super) fn list_merge_states(registry: &Registry) -> Result<Vec<MergeState>> {
    let state = registry.state()?;
    let mut merges: Vec<MergeState> = state.merge_states.into_values().collect();
    merges.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merges.reverse();
    Ok(merges)
}

pub(super) fn list_ui_events(registry: &Registry, limit: usize) -> Result<Vec<UiEvent>> {
    let events = registry.list_events(None, limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();
    Ok(ui_events)
}

#[allow(dead_code)]
pub(super) fn list_runtime_stream_items(
    registry: &Registry,
    flow_id: Option<Uuid>,
    attempt_id: Option<Uuid>,
    limit: usize,
) -> Result<Vec<RuntimeStreamItemView>> {
    let mut filter = EventFilter::all();
    filter.flow_id = flow_id;
    filter.attempt_id = attempt_id;
    filter.limit = Some(limit);
    let mut items = registry
        .read_events(&filter)?
        .into_iter()
        .filter_map(runtime_stream_item)
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.sequence);
    Ok(items)
}

#[allow(clippy::too_many_lines)]
pub(super) fn runtime_stream_item(event: Event) -> Option<RuntimeStreamItemView> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_stream_item_maps_turn_event() {
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let mut event = Event::new(
            EventPayload::RuntimeTurnCompleted {
                attempt_id,
                adapter_name: "opencode".to_string(),
                stream: RuntimeOutputStream::Stdout,
                ordinal: 2,
                provider_session_id: Some("sess-1".to_string()),
                provider_turn_id: Some("snap-2".to_string()),
                git_ref: Some("refs/hivemind/transient/turns/task/attempt/turn-0002".to_string()),
                commit_sha: Some("abc123".to_string()),
                summary: Some("Turn complete".to_string()),
            },
            CorrelationIds {
                project_id: None,
                graph_id: None,
                flow_id: Some(flow_id),
                workflow_id: None,
                workflow_run_id: None,
                root_workflow_run_id: None,
                parent_workflow_run_id: None,
                task_id: Some(task_id),
                step_id: None,
                step_run_id: None,
                attempt_id: Some(attempt_id),
            },
        );
        event.metadata.sequence = Some(42);

        let item = runtime_stream_item(event).expect("runtime stream item");
        assert_eq!(item.kind, "turn");
        assert_eq!(item.sequence, 42);
        assert_eq!(item.flow_id.as_deref(), Some(flow_id.to_string().as_str()));
        assert_eq!(
            item.attempt_id.as_deref(),
            Some(attempt_id.to_string().as_str())
        );
        assert_eq!(item.data["ordinal"], 2);
        assert_eq!(
            item.data["git_ref"],
            "refs/hivemind/transient/turns/task/attempt/turn-0002"
        );
    }
}
