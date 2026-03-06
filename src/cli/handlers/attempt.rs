//! Attempt command handlers.

use crate::cli::commands::{AttemptCommands, AttemptInspectArgs, AttemptListArgs};
use crate::cli::handlers::common::get_registry;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError, Result};
use crate::core::registry::Registry;
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
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        AttemptCommands::List(args) => list::handle_attempt_list(&registry, &args, format),
        AttemptCommands::Inspect(args) => handle_attempt_inspect(&registry, &args, format),
    }
}
