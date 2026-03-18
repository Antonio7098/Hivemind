use super::*;
use crate::app::{EventService, StateService};
use crate::core::events::RuntimeOutputStream;
pub(super) mod stream;
pub(crate) use stream::runtime_stream_item;
use uuid::Uuid;

pub(super) fn list_tasks(state_service: &StateService) -> Result<Vec<Task>> {
    let state = state_service.state()?;
    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();
    Ok(tasks)
}

pub(super) fn list_graphs(state_service: &StateService) -> Result<Vec<TaskGraph>> {
    let state = state_service.state()?;
    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();
    Ok(graphs)
}

pub(super) fn list_flows(state_service: &StateService) -> Result<Vec<TaskFlow>> {
    let state = state_service.state()?;
    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();
    Ok(flows)
}

pub(super) fn list_merge_states(state_service: &StateService) -> Result<Vec<MergeState>> {
    let state = state_service.state()?;
    let mut merges: Vec<MergeState> = state.merge_states.into_values().collect();
    merges.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merges.reverse();
    Ok(merges)
}

pub(super) fn list_ui_events(event_service: &EventService, limit: usize) -> Result<Vec<UiEvent>> {
    let events = event_service.list_events(None, limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();
    Ok(ui_events)
}

#[allow(dead_code)]
pub(super) fn list_runtime_stream_items(
    event_service: &EventService,
    flow_id: Option<Uuid>,
    attempt_id: Option<Uuid>,
    limit: usize,
) -> Result<Vec<RuntimeStreamItemView>> {
    let mut filter = EventFilter::all();
    filter.flow_id = flow_id;
    filter.attempt_id = attempt_id;
    filter.limit = Some(limit);
    let mut items = event_service
        .read_events(&filter)?
        .into_iter()
        .filter_map(runtime_stream_item)
        .collect::<Vec<_>>();
    items.sort_by_key(|item| item.sequence);
    Ok(items)
}

// extracted stream items

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
                task_id: Some(task_id),
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
