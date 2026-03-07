use super::*;
use crate::storage::event_store::EventFilter;

pub(crate) fn handle_attempt_inspect(
    registry: &Registry,
    args: &AttemptInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    let attempt_id = &args.attempt_id;
    let Ok(parsed) = Uuid::parse_str(attempt_id) else {
        return output_error(
            &HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "cli:attempt:inspect",
            ),
            format,
        );
    };

    match try_print_attempt_from_events(registry, parsed, args, format) {
        Ok(Some(code)) => return code,
        Ok(None) => {}
        Err(e) => return output_error(&e, format),
    }

    match registry.get_attempt(attempt_id) {
        Ok(attempt) => {
            print_attempt_inspect_attempt(registry, &attempt, args.diff, args.context, format)
        }
        Err(e) if e.code == "attempt_not_found" => {
            print_attempt_inspect_task_fallback(registry, parsed, args.diff, format, &e)
        }
        Err(e) => output_error(&e, format),
    }
}

fn try_print_attempt_from_events(
    registry: &Registry,
    attempt_id: Uuid,
    args: &AttemptInspectArgs,
    format: OutputFormat,
) -> Result<Option<ExitCode>> {
    let mut filter = EventFilter::all();
    filter.attempt_id = Some(attempt_id);
    let events = registry.read_events(&filter)?;
    if events.is_empty() {
        return Ok(None);
    }

    let corr = events[0].metadata.correlation.clone();
    let collected = collect_attempt_runtime_data(&events);
    let info = build_attempt_inspect_json(attempt_id, &corr, &collected, args);

    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            print_attempt_inspect_table(attempt_id, &corr, &collected, args);
            if args.diff {
                if let Some(diff) = registry.get_attempt_diff(&attempt_id.to_string())? {
                    println!("{diff}");
                }
            }
        }
    }

    Ok(Some(ExitCode::Success))
}

fn print_attempt_inspect_task_fallback(
    registry: &Registry,
    task_id: Uuid,
    show_diff: bool,
    format: OutputFormat,
    original_error: &HivemindError,
) -> ExitCode {
    let state = match registry.state() {
        Ok(s) => s,
        Err(e) => return output_error(&e, format),
    };

    let exec_info = state
        .flows
        .values()
        .find_map(|flow| flow.task_executions.get(&task_id).map(|exec| (flow, exec)));

    let Some((flow, exec)) = exec_info else {
        return output_error(original_error, format);
    };

    let latest_attempt = state
        .attempts
        .values()
        .filter(|a| a.task_id == task_id && a.flow_id == flow.id)
        .max_by_key(|a| a.started_at);

    let diff = if show_diff {
        latest_attempt
            .and_then(|a| a.diff_id.map(|_| a.id))
            .and_then(|aid| registry.get_attempt_diff(&aid.to_string()).ok())
            .flatten()
    } else {
        None
    };

    let info = serde_json::json!({
        "task_id": exec.task_id,
        "state": format!("{:?}", exec.state),
        "attempt_count": exec.attempt_count,
        "blocked_reason": exec.blocked_reason,
        "flow_id": flow.id,
        "diff": diff,
    });

    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Task:     {}", exec.task_id);
            println!("Flow:     {}", flow.id);
            println!("State:    {:?}", exec.state);
            println!("Attempts: {}", exec.attempt_count);
            if let Some(ref reason) = exec.blocked_reason {
                println!("Blocked:  {reason}");
            }
            if let Some(d) = info.get("diff").and_then(|v| v.as_str()) {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}
