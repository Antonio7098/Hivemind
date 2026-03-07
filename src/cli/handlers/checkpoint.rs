//! Checkpoint command handlers.

use crate::cli::commands::CheckpointCommands;
use crate::cli::handlers::common::get_registry;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;

pub fn handle_checkpoint(cmd: CheckpointCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        CheckpointCommands::List(args) => match registry.list_checkpoints(&args.attempt_id) {
            Ok(checkpoints) => match format {
                OutputFormat::Table => {
                    if checkpoints.is_empty() {
                        println!("No checkpoints declared for this attempt.");
                    } else {
                        println!(
                            "{:<32}  {:<9}  {:<11}  {:<12}  SUMMARY",
                            "CHECKPOINT", "ORDER", "STATE", "COMMIT"
                        );
                        println!("{}", "-".repeat(120));
                        for checkpoint in checkpoints {
                            let commit = checkpoint.commit_hash.unwrap_or_else(|| "-".to_string());
                            let summary = checkpoint.summary.unwrap_or_default();
                            println!(
                                "{:<32}  {:<9}  {:<11}  {:<12}  {}",
                                checkpoint.checkpoint_id,
                                format!("{}/{}", checkpoint.order, checkpoint.total),
                                format!("{:?}", checkpoint.state).to_lowercase(),
                                commit,
                                summary
                            );
                        }
                    }
                    ExitCode::Success
                }
                _ => output(&checkpoints, format)
                    .map(|()| ExitCode::Success)
                    .unwrap_or(ExitCode::Error),
            },
            Err(e) => output_error(&e, format),
        },
        CheckpointCommands::Complete(args) => {
            let attempt_id = args
                .attempt_id
                .or_else(|| std::env::var("HIVEMIND_ATTEMPT_ID").ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let Some(attempt_id) = attempt_id else {
                return output_error(
                    &crate::core::error::HivemindError::user(
                        "attempt_id_required",
                        "Attempt ID is required (pass --attempt-id or set HIVEMIND_ATTEMPT_ID)",
                        "cli:checkpoint:complete",
                    ),
                    format,
                );
            };

            match registry.checkpoint_complete(
                &attempt_id,
                &args.checkpoint_id,
                args.summary.as_deref(),
            ) {
                Ok(result) => {
                    match format {
                        OutputFormat::Json | OutputFormat::Yaml => {
                            if let Err(err) = output(&result, format) {
                                eprintln!("Failed to render checkpoint result: {err}");
                            }
                        }
                        OutputFormat::Table => {
                            println!("Attempt:    {}", result.attempt_id);
                            println!(
                                "Checkpoint: {} ({}/{})",
                                result.checkpoint_id, result.order, result.total
                            );
                            println!("Commit:     {}", result.commit_hash);
                            if let Some(next) = result.next_checkpoint_id {
                                println!("Next:       {next}");
                            } else {
                                println!("Next:       none");
                            }
                            println!("All done:   {}", result.all_completed);
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
