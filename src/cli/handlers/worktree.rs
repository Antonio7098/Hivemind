//! Worktree command handlers.

use crate::cli::commands::WorktreeCommands;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;

fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn handle_worktree(cmd: WorktreeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        WorktreeCommands::List(args) => match registry.worktree_list(&args.flow_id) {
            Ok(statuses) => match format {
                OutputFormat::Json => {
                    let response = crate::cli::output::CliResponse::success(&statuses);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    let response = crate::cli::output::CliResponse::success(&statuses);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&statuses) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        WorktreeCommands::Inspect(args) => match registry.worktree_inspect(&args.task_id) {
            Ok(status) => match format {
                OutputFormat::Json => {
                    let response = crate::cli::output::CliResponse::success(&status);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    let response = crate::cli::output::CliResponse::success(&status);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&status) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        WorktreeCommands::Cleanup(args) => {
            match registry.worktree_cleanup(&args.flow_id, args.force, args.dry_run) {
                Ok(result) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "success": true,
                                    "flow_id": result.flow_id,
                                    "force": result.forced,
                                    "dry_run": result.dry_run,
                                    "cleaned_worktrees": result.cleaned_worktrees
                                })
                            );
                        }
                        OutputFormat::Yaml => {
                            if let Ok(yaml) = serde_yaml::to_string(&serde_json::json!({
                                "success": true,
                                "flow_id": result.flow_id,
                                "force": result.forced,
                                "dry_run": result.dry_run,
                                "cleaned_worktrees": result.cleaned_worktrees
                            })) {
                                print!("{yaml}");
                            }
                        }
                        OutputFormat::Table => {
                            if result.dry_run {
                                println!(
                                    "Dry run complete. {} worktree(s) would be cleaned.",
                                    result.cleaned_worktrees
                                );
                            } else {
                                println!(
                                    "Cleanup complete. {} worktree(s) cleaned.",
                                    result.cleaned_worktrees
                                );
                            }
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
