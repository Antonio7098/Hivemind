use super::*;

pub(super) fn handle_attempt_list(
    registry: &Registry,
    args: &AttemptListArgs,
    format: OutputFormat,
) -> ExitCode {
    match registry.list_attempts(args.flow.as_deref(), args.task.as_deref(), args.limit) {
        Ok(attempts) => match format {
            OutputFormat::Table => {
                if attempts.is_empty() {
                    println!("No attempts found.");
                } else {
                    println!(
                        "{:<36}  {:<36}  {:<36}  {:<8}  CHECKPOINTS",
                        "ATTEMPT", "FLOW", "TASK", "NUMBER"
                    );
                    println!("{}", "-".repeat(170));
                    for attempt in attempts {
                        println!(
                            "{:<36}  {:<36}  {:<36}  {:<8}  {}",
                            attempt.attempt_id,
                            attempt.flow_id,
                            attempt.task_id,
                            attempt.attempt_number,
                            if attempt.all_checkpoints_completed {
                                "all_completed"
                            } else {
                                "incomplete"
                            }
                        );
                    }
                }
                ExitCode::Success
            }
            _ => output(&attempts, format)
                .map(|()| ExitCode::Success)
                .unwrap_or(ExitCode::Error),
        },
        Err(e) => output_error(&e, format),
    }
}
