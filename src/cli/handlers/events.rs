//! Events command handlers.

use crate::cli::commands::{
    EventCommands, EventInspectArgs, EventListArgs, EventNativeSummaryArgs, EventRecoverArgs,
    EventReplayArgs, EventStreamArgs,
};
use crate::cli::handlers::common::{get_registry, print_structured};
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;

mod commands;
mod filter;
mod labels;
mod native_summary;
mod redact;
mod render;

pub fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => commands::handle_events_list(&registry, &args, format),
        EventCommands::Inspect(args) => commands::handle_events_inspect(&registry, &args, format),
        EventCommands::Stream(args) => commands::handle_events_stream(&registry, &args, format),
        EventCommands::NativeSummary(args) => {
            native_summary::handle_events_native_summary(&registry, &args, format)
        }
        EventCommands::Replay(args) => commands::handle_events_replay(&registry, &args, format),
        EventCommands::Verify(_args) => commands::handle_events_verify(&registry, format),
        EventCommands::Recover(args) => commands::handle_events_recover(&registry, &args, format),
    }
}
