//! Verify command handlers.

use crate::cli::commands::VerifyCommands;
use crate::cli::handlers::common::{get_registry, print_flow_id};
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;

pub fn handle_verify(cmd: VerifyCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        VerifyCommands::Override(args) => {
            match registry.verify_override(&args.task_id, &args.decision, &args.reason) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        VerifyCommands::Run(args) => match registry.verify_run(&args.task_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        VerifyCommands::Results(args) => match registry.get_attempt(&args.attempt_id) {
            Ok(attempt) => {
                let check_results: Vec<serde_json::Value> = attempt
                    .check_results
                    .iter()
                    .map(|r| {
                        if args.output {
                            serde_json::json!(r)
                        } else {
                            serde_json::json!({
                                "name": r.name,
                                "passed": r.passed,
                                "exit_code": r.exit_code,
                                "duration_ms": r.duration_ms,
                                "required": r.required,
                            })
                        }
                    })
                    .collect();

                let info = serde_json::json!({
                    "attempt_id": attempt.id,
                    "task_id": attempt.task_id,
                    "flow_id": attempt.flow_id,
                    "attempt_number": attempt.attempt_number,
                    "check_results": check_results,
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
                        if attempt.check_results.is_empty() {
                            println!("No check results found.");
                        } else {
                            println!("Checks:");
                            for r in &attempt.check_results {
                                let status = if r.passed { "PASS" } else { "FAIL" };
                                let required = if r.required { "required" } else { "optional" };
                                println!(
                                    "  - {}: {} ({required}) exit={} duration={}ms",
                                    r.name, status, r.exit_code, r.duration_ms
                                );
                                if args.output && !r.output.trim().is_empty() {
                                    println!("    Output:\n{}", r.output.trim_end());
                                }
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
