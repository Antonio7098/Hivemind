use super::labels::event_type_label;
use super::*;
use crate::cli::output::CliResponse;
use crate::core::flow::TaskFlow;

pub(super) fn print_events_table(events: Vec<crate::core::events::Event>) {
    if events.is_empty() {
        println!("No events found.");
        return;
    }
    println!("{:<6}  {:<24}  {:<22}  PROJECT", "SEQ", "TYPE", "TIMESTAMP");
    println!("{}", "-".repeat(90));
    for ev in events {
        let seq = ev.metadata.sequence.unwrap_or(0);
        let typ = event_type_label(&ev.payload);
        let ts = ev.metadata.timestamp.to_rfc3339();
        let proj = ev
            .metadata
            .correlation
            .project_id
            .map_or_else(|| "-".to_string(), |p| p.to_string());
        println!("{seq:<6}  {typ:<24}  {ts:<22}  {proj}");
    }
}

pub(super) fn print_events_response(events: &[crate::core::events::Event], format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let response = CliResponse::success(events);
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = CliResponse::success(events);
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => print_events_table(events.to_vec()),
    }
}

pub(super) fn print_event_payload(event: &crate::core::events::Event, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let response = CliResponse::success(event);
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(event) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(event) {
                println!("{json}");
            }
        }
    }
}

pub(super) fn print_flow_replay(replayed: &TaskFlow, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(replayed) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(replayed) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("ID:      {}", replayed.id);
            println!("Graph:   {}", replayed.graph_id);
            println!("State:   {:?}", replayed.state);
            let mut counts = std::collections::HashMap::new();
            for exec in replayed.task_executions.values() {
                *counts.entry(exec.state).or_insert(0usize) += 1;
            }
            println!("Tasks:");
            let mut keys: Vec<_> = counts.keys().copied().collect();
            keys.sort_by_key(|k| format!("{k:?}"));
            for k in keys {
                println!("  - {:?}: {}", k, counts[&k]);
            }
        }
    }
}
