use super::*;
mod types;
use types::*;
mod categories;
use categories::*;
mod payload;
use payload::*;

pub(super) fn ui_event(event: &Event) -> Result<UiEvent> {
    Ok(UiEvent {
        id: event.metadata.id.to_string(),
        r#type: payload_pascal_type(&event.payload).to_string(),
        category: payload_category(&event.payload).to_string(),
        timestamp: event.metadata.timestamp,
        sequence: event.metadata.sequence,
        correlation: event.metadata.correlation.clone(),
        payload: payload_map(&event.payload)?,
    })
}
pub(super) fn build_ui_state(registry: &Registry, events_limit: usize) -> Result<UiState> {
    let state = registry.state()?;

    let mut projects: Vec<Project> = state.projects.into_values().collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();

    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();

    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();

    let mut merge_states: Vec<MergeState> = state.merge_states.into_values().collect();
    merge_states.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merge_states.reverse();

    let events = registry.list_events(None, events_limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();

    Ok(UiState {
        projects,
        tasks,
        graphs,
        flows,
        merge_states,
        events: ui_events,
    })
}
