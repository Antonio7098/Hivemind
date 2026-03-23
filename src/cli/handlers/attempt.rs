//! Attempt command handlers.

use crate::app::AttemptService;
use crate::cli::commands::{AttemptCommands, AttemptInspectArgs, AttemptListArgs};
use crate::cli::handlers::common::get_attempt_service;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError, Result};
use crate::core::state::AttemptState;
use uuid::Uuid;

mod inspect;
mod list;
mod render;
mod runtime_data;

pub(crate) use inspect::handle_attempt_inspect;
pub(crate) use render::{
    build_attempt_inspect_json, print_attempt_inspect_attempt, print_attempt_inspect_table,
};
pub(crate) use runtime_data::{
    attempt_context_from_events, collect_attempt_runtime_data, AttemptInspectCollected,
};

pub fn handle_attempt(cmd: AttemptCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_attempt_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        AttemptCommands::List(args) => list::handle_attempt_list(&service, &args, format),
        AttemptCommands::Inspect(args) => handle_attempt_inspect(&service, &args, format),
    }
}
