//! Runtime command handlers.

use crate::cli::commands::{RuntimeCommands, RuntimeRoleArg};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::events::RuntimeRole;
use crate::core::registry::Registry;

fn parse_runtime_role(role: RuntimeRoleArg) -> RuntimeRole {
    match role {
        RuntimeRoleArg::Worker => RuntimeRole::Worker,
        RuntimeRoleArg::Validator => RuntimeRole::Validator,
    }
}

fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub fn handle_runtime(cmd: RuntimeCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        RuntimeCommands::List => {
            let rows = registry.runtime_list();
            match format {
                OutputFormat::Table => {
                    if rows.is_empty() {
                        println!("No runtimes available.");
                    } else {
                        println!(
                            "{:<14}  {:<20}  {:<10}  {:<12}  CAPABILITIES",
                            "ADAPTER", "BINARY", "AVAILABLE", "OCODE_FAMILY"
                        );
                        println!("{}", "-".repeat(120));
                        for row in rows {
                            println!(
                                "{:<14}  {:<20}  {:<10}  {:<12}  {}",
                                row.adapter_name,
                                row.default_binary,
                                if row.available { "yes" } else { "no" },
                                if row.opencode_compatible { "yes" } else { "no" },
                                row.capabilities.join(",")
                            );
                        }
                    }
                    ExitCode::Success
                }
                _ => output(&rows, format)
                    .map(|()| ExitCode::Success)
                    .unwrap_or(ExitCode::Error),
            }
        }
        RuntimeCommands::Health(args) => {
            match registry.runtime_health_with_role(
                args.project.as_deref(),
                args.task.as_deref(),
                args.flow.as_deref(),
                parse_runtime_role(args.role),
            ) {
                Ok(status) => {
                    match format {
                        OutputFormat::Table => {
                            println!("Adapter: {}", status.adapter_name);
                            println!("Binary: {}", status.binary_path);
                            println!("Healthy: {}", if status.healthy { "yes" } else { "no" });
                            if !status.capabilities.is_empty() {
                                println!("Capabilities: {}", status.capabilities.join(", "));
                            }
                            if let Some(selection_source) = status.selection_source {
                                println!("Selection Source: {}", selection_source.as_str());
                            }
                            if let Some(ref target) = status.target {
                                println!("Target: {target}");
                            }
                            if let Some(ref details) = status.details {
                                println!("Details: {details}");
                            }
                        }
                        _ => {
                            if output(&status, format).is_err() {
                                return ExitCode::Error;
                            }
                        }
                    }
                    if status.healthy {
                        ExitCode::Success
                    } else {
                        ExitCode::Error
                    }
                }
                Err(e) => output_error(&e, format),
            }
        }
        RuntimeCommands::DefaultsSet(args) => match registry.runtime_defaults_set(
            parse_runtime_role(args.role),
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
            args.max_parallel_tasks,
        ) {
            Ok(()) => {
                if matches!(format, OutputFormat::Table) {
                    println!("ok");
                }
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
