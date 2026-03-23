use crate::app::WorktreeService;
use crate::cli::commands::WorktreeCommands;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;

// ARCH_DEBT: legacy oversized function awaiting CLI handler refactor
#[allow(clippy::too_many_lines)]
pub(super) fn handle_worktree_core(
    service: &WorktreeService,
    cmd: WorktreeCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        WorktreeCommands::List(args) => match service.worktree_list(&args.flow_id) {
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
        WorktreeCommands::Inspect(args) => match service.worktree_inspect(&args.task_id) {
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
            match service.worktree_cleanup(&args.flow_id, args.force, args.dry_run) {
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
        WorktreeCommands::RestoreTurn(args) => {
            match service.worktree_restore_turn_ref(
                &args.attempt_id,
                args.ordinal,
                args.confirm,
                args.force,
            ) {
                Ok(result) => {
                    match format {
                        OutputFormat::Json => {
                            let response = crate::cli::output::CliResponse::success(&result);
                            if let Ok(json) = serde_json::to_string_pretty(&response) {
                                println!("{json}");
                            }
                        }
                        OutputFormat::Yaml => {
                            let response = crate::cli::output::CliResponse::success(&result);
                            if let Ok(yaml) = serde_yaml::to_string(&response) {
                                print!("{yaml}");
                            }
                        }
                        OutputFormat::Table => {
                            println!(
                                "Restored attempt {} turn {} into {}",
                                result.attempt_id,
                                result.ordinal,
                                result.worktree_path.display()
                            );
                            println!("Task:     {}", result.task_id);
                            println!("Flow:     {}", result.flow_id);
                            if let Some(branch) = result.branch.as_deref() {
                                println!("Branch:   {branch}");
                            }
                            if let Some(git_ref) = result.git_ref.as_deref() {
                                println!("Ref:      {git_ref}");
                            }
                            if let Some(commit_sha) = result.commit_sha.as_deref() {
                                println!("Commit:   {commit_sha}");
                            }
                            println!("HEAD:     {}", result.head_after);
                            println!("Dirty:    {}", result.had_local_changes);
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
