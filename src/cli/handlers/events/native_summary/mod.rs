use crate::app::EventService;
use crate::cli::commands::EventNativeSummaryArgs;
use crate::cli::handlers::common::print_structured;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;

use super::filter::build_event_filter;

mod builder;
mod types;

use builder::build_native_summary;

pub(super) fn handle_events_native_summary(
    service: &EventService,
    args: &EventNativeSummaryArgs,
    format: OutputFormat,
) -> ExitCode {
    let filter = match build_event_filter(
        service,
        "cli:events:native-summary",
        args.project.as_deref(),
        args.graph.as_deref(),
        args.flow.as_deref(),
        None,
        None,
        None,
        None,
        None,
        None,
        args.task.as_deref(),
        args.attempt.as_deref(),
        args.artifact_id.as_deref(),
        args.template_id.as_deref(),
        args.rule_id.as_deref(),
        args.error_type.as_deref(),
        args.since.as_deref(),
        args.until.as_deref(),
        args.limit,
    ) {
        Ok(filter) => filter,
        Err(error) => return output_error(&error, format),
    };

    let events = match service.read_events(&filter) {
        Ok(events) => events,
        Err(error) => return output_error(&error, format),
    };

    let report = build_native_summary(&events);
    let passed = report.verification.failures.is_empty();
    print_structured(&report, format, "native event summary");
    if args.verify && !passed {
        ExitCode::Error
    } else {
        ExitCode::Success
    }
}

#[cfg(test)]
mod tests;
