use super::*;
use crate::core::events::CorrelationIds;

pub(crate) mod inspect;

pub(crate) use inspect::json::build_attempt_inspect_json;
pub(crate) use inspect::table::print_attempt_inspect_table;

pub(crate) fn print_attempt_inspect_attempt(
    service: &AttemptService,
    attempt: &AttemptState,
    show_diff: bool,
    show_context: bool,
    format: OutputFormat,
) -> ExitCode {
    let diff = if show_diff {
        match service.get_attempt_diff(&attempt.id.to_string()) {
            Ok(d) => d,
            Err(e) => return output_error(&e, format),
        }
    } else {
        None
    };
    let context_value = if show_context {
        attempt_context_from_events(service, attempt.id)
    } else {
        None
    };

    let info = serde_json::json!({
        "attempt_id": attempt.id,
        "task_id": attempt.task_id,
        "flow_id": attempt.flow_id,
        "attempt_number": attempt.attempt_number,
        "started_at": attempt.started_at,
        "baseline_id": attempt.baseline_id,
        "diff_id": attempt.diff_id,
        "runtime_session": attempt.runtime_session,
        "turn_refs": attempt.turn_refs,
        "diff": diff,
        "context": context_value,
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
            println!("Attempt:  {}", attempt.id);
            println!("Task:     {}", attempt.task_id);
            println!("Flow:     {}", attempt.flow_id);
            println!("Number:   {}", attempt.attempt_number);
            println!("Started:  {}", attempt.started_at);
            if let Some(b) = attempt.baseline_id {
                println!("Baseline: {b}");
            }
            if let Some(did) = attempt.diff_id {
                println!("Diff:     {did}");
            }
            if let Some(ctx) = context_value {
                if let Ok(rendered) = serde_json::to_string_pretty(&ctx) {
                    println!("Context:\n{rendered}");
                }
            }
            if let Some(session) = attempt.runtime_session.as_ref() {
                println!(
                    "Runtime:  {} session {} ({})",
                    session.adapter_name, session.session_id, session.discovered_at
                );
            }
            if !attempt.turn_refs.is_empty() {
                println!("Turn refs:");
                for turn in &attempt.turn_refs {
                    println!(
                        "  - turn {} [{}] ref={} commit={}",
                        turn.ordinal,
                        format!("{:?}", turn.stream).to_lowercase(),
                        turn.git_ref.as_deref().unwrap_or("-"),
                        turn.commit_sha.as_deref().unwrap_or("-")
                    );
                }
            }
            if let Some(d) = diff {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}
