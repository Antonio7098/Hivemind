//! Merge command handlers.

use crate::cli::commands::MergeCommands;
use crate::cli::handlers::common::{get_registry, parse_merge_execute_mode};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::MergeExecuteOptions;

pub fn handle_merge(cmd: MergeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        MergeCommands::Prepare(args) => {
            match registry.merge_prepare(&args.flow_id, args.target.as_deref()) {
                Ok(ms) => {
                    match format {
                        OutputFormat::Json => {
                            if let Ok(json) = serde_json::to_string_pretty(&ms) {
                                println!("{json}");
                            }
                        }
                        OutputFormat::Yaml => {
                            if let Ok(yaml) = serde_yaml::to_string(&ms) {
                                print!("{yaml}");
                            }
                        }
                        OutputFormat::Table => {
                            println!("Flow:     {}", ms.flow_id);
                            println!("Status:   {:?}", ms.status);
                            if let Some(ref branch) = ms.target_branch {
                                println!("Target:   {branch}");
                            }
                            if !ms.conflicts.is_empty() {
                                println!("Conflicts:");
                                for c in &ms.conflicts {
                                    println!("  - {c}");
                                }
                            }
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        MergeCommands::Approve(args) => match registry.merge_approve(&args.flow_id) {
            Ok(ms) => {
                match format {
                    OutputFormat::Json => {
                        if let Ok(json) = serde_json::to_string_pretty(&ms) {
                            println!("{json}");
                        }
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&ms) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("Flow:   {}", ms.flow_id);
                        println!("Status: {:?}", ms.status);
                    }
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        MergeCommands::Execute(args) => match registry.merge_execute_with_options(
            &args.flow_id,
            MergeExecuteOptions {
                mode: parse_merge_execute_mode(args.mode),
                monitor_ci: args.monitor_ci,
                auto_merge: args.auto_merge,
                pull_after: args.pull_after,
            },
        ) {
            Ok(ms) => {
                match format {
                    OutputFormat::Json => {
                        if let Ok(json) = serde_json::to_string_pretty(&ms) {
                            println!("{json}");
                        }
                    }
                    OutputFormat::Yaml => {
                        if let Ok(yaml) = serde_yaml::to_string(&ms) {
                            print!("{yaml}");
                        }
                    }
                    OutputFormat::Table => {
                        println!("Flow:   {}", ms.flow_id);
                        println!("Status: {:?}", ms.status);
                        if !ms.commits.is_empty() {
                            println!("Commits:");
                            for c in &ms.commits {
                                println!("  - {c}");
                            }
                        }
                    }
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
