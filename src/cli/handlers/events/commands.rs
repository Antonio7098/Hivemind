use super::filter::build_event_filter;
use super::redact::redact_events_for_stream;
use super::render::{print_event_payload, print_events_response, print_flow_replay};
use super::*;

pub(super) fn handle_events_list(
    registry: &Registry,
    args: &EventListArgs,
    format: OutputFormat,
) -> ExitCode {
    let filter = match build_event_filter(
        registry,
        "cli:events:list",
        args.project.as_deref(),
        args.graph.as_deref(),
        args.flow.as_deref(),
        args.workflow.as_deref(),
        args.workflow_run.as_deref(),
        args.root_workflow_run.as_deref(),
        args.parent_workflow_run.as_deref(),
        args.step.as_deref(),
        args.step_run.as_deref(),
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
        Ok(f) => f,
        Err(e) => return output_error(&e, format),
    };

    let events = match registry.read_events(&filter) {
        Ok(evs) => evs,
        Err(e) => return output_error(&e, format),
    };

    print_events_response(&events, format);
    ExitCode::Success
}

pub(super) fn handle_events_inspect(
    registry: &Registry,
    args: &EventInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    let event = match registry.get_event(&args.event_id) {
        Ok(ev) => ev,
        Err(e) => return output_error(&e, format),
    };

    print_event_payload(&event, format);
    ExitCode::Success
}

pub(super) fn handle_events_stream(
    registry: &Registry,
    args: &EventStreamArgs,
    format: OutputFormat,
) -> ExitCode {
    let filter = match build_event_filter(
        registry,
        "cli:events:stream",
        args.project.as_deref(),
        args.graph.as_deref(),
        args.flow.as_deref(),
        args.workflow.as_deref(),
        args.workflow_run.as_deref(),
        args.root_workflow_run.as_deref(),
        args.parent_workflow_run.as_deref(),
        args.step.as_deref(),
        args.step_run.as_deref(),
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
        Ok(f) => f,
        Err(e) => return output_error(&e, format),
    };

    let rx = match registry.stream_events(&filter) {
        Ok(r) => r,
        Err(e) => return output_error(&e, format),
    };

    let mut events = Vec::new();
    let idle_timeout = std::time::Duration::from_millis(1000);
    while let Ok(ev) = rx.recv_timeout(idle_timeout) {
        events.push(ev);
        if events.len() >= args.limit {
            break;
        }
    }
    if args.redact_secrets {
        events = redact_events_for_stream(events);
    }

    print_events_response(&events, format);
    ExitCode::Success
}

pub(super) fn handle_events_replay(
    registry: &Registry,
    args: &EventReplayArgs,
    format: OutputFormat,
) -> ExitCode {
    let replayed = match registry.replay_flow(&args.flow_id) {
        Ok(f) => f,
        Err(e) => return output_error(&e, format),
    };

    if args.verify {
        let current = match registry.get_flow(&args.flow_id) {
            Ok(f) => f,
            Err(e) => return output_error(&e, format),
        };

        let match_ok = replayed.state == current.state
            && replayed.task_executions.len() == current.task_executions.len()
            && replayed.task_executions.iter().all(|(tid, exec)| {
                current
                    .task_executions
                    .get(tid)
                    .is_some_and(|ce| ce.state == exec.state)
            });

        if match_ok {
            println!("Verification passed: replayed state matches current state.");
        } else {
            eprintln!("Verification FAILED: replayed state differs from current state.");
            return ExitCode::Error;
        }
    }

    print_flow_replay(&replayed, format);
    ExitCode::Success
}

pub(super) fn handle_events_verify(registry: &Registry, format: OutputFormat) -> ExitCode {
    match registry.events_verify() {
        Ok(result) => {
            print_structured(&result, format, "events verify result");
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}

pub(super) fn handle_events_recover(
    registry: &Registry,
    args: &EventRecoverArgs,
    format: OutputFormat,
) -> ExitCode {
    if !args.from_mirror {
        return output_error(
            &crate::core::error::HivemindError::user(
                "events_recover_source_required",
                "Specify an explicit source for recovery",
                "cli:events:recover",
            )
            .with_hint("Use `hivemind events recover --from-mirror --confirm`"),
            format,
        );
    }

    match registry.events_recover_from_mirror(args.confirm) {
        Ok(result) => {
            print_structured(&result, format, "events recover result");
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
