//! Events command handlers.

use crate::app::EventService;
use crate::cli::commands::{
    EventCommands, EventInspectArgs, EventListArgs, EventNativeSummaryArgs, EventRecoverArgs,
    EventReplayArgs, EventStreamArgs,
};
use crate::cli::handlers::common::{get_event_service, print_structured};
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;

mod commands;
mod filter;
mod labels;
mod native_summary;
mod redact;
mod render;

pub fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_event_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => commands::handle_events_list(&service, &args, format),
        EventCommands::Inspect(args) => commands::handle_events_inspect(&service, &args, format),
        EventCommands::Stream(args) => commands::handle_events_stream(&service, &args, format),
        EventCommands::NativeSummary(args) => {
            native_summary::handle_events_native_summary(&service, &args, format)
        }
        EventCommands::Replay(args) => commands::handle_events_replay(&service, &args, format),
        EventCommands::Verify(_args) => commands::handle_events_verify(&service, format),
        EventCommands::Recover(args) => commands::handle_events_recover(&service, &args, format),
    }
}
