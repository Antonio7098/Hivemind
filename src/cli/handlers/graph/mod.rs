use super::common::get_graph_service;
use crate::cli::commands::{GraphCommands, GraphQueryCommands, GraphSnapshotCommands};
use crate::cli::output::OutputFormat;
use crate::core::error::ExitCode;

pub(super) mod core;
pub(super) mod query;
pub(super) mod snapshot;
pub(super) mod view;

// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
pub fn handle_graph(cmd: GraphCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_graph_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GraphCommands::Query(cmd) => query::handle_graph_query(cmd, format),
        GraphCommands::Snapshot(cmd) => snapshot::handle_graph_snapshot(cmd, format),
        _ => core::handle_graph_core(&service, cmd, format),
    }
}
