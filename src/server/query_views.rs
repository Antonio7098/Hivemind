use super::*;

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
