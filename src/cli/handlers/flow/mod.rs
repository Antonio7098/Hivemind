use super::common::{get_flow_service, parse_run_mode, parse_runtime_role, print_flow_id};
use crate::cli::commands::FlowCommands;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use uuid::Uuid;

pub(super) mod core;
pub(super) mod view;

// ARCH_DEBT: oversized unit retained temporarily while checklist-driven extraction continues.
#[allow(clippy::too_many_lines)]
pub fn handle_flow(cmd: FlowCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_flow_service(format) else {
        return ExitCode::Error;
    };

    core::handle_flow_core(&service, cmd, format)
}
