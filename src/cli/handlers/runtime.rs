//! Runtime command handlers.

use crate::cli::commands::{RuntimeCommands, RuntimeRoleArg, RuntimeStreamArgs};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::events::RuntimeRole;
use crate::core::registry::shared_types::RuntimeStreamDetailLevel;
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

fn parse_runtime_stream_detail(
    detail: crate::cli::commands::RuntimeStreamDetailArg,
) -> RuntimeStreamDetailLevel {
    match detail {
        crate::cli::commands::RuntimeStreamDetailArg::Summary => RuntimeStreamDetailLevel::Summary,
        crate::cli::commands::RuntimeStreamDetailArg::Observability => {
            RuntimeStreamDetailLevel::Observability
        }
        crate::cli::commands::RuntimeStreamDetailArg::Telemetry => {
            RuntimeStreamDetailLevel::Telemetry
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
        RuntimeCommands::Stream(args) => handle_runtime_stream(&registry, &args, format),
    }
}

fn handle_runtime_stream(
    registry: &Registry,
    args: &RuntimeStreamArgs,
    format: OutputFormat,
) -> ExitCode {
    match registry.runtime_stream_items_with_detail(
        args.flow.as_deref(),
        args.attempt.as_deref(),
        args.limit,
        parse_runtime_stream_detail(args.detail),
    ) {
        Ok(items) => match format {
            OutputFormat::Table => {
                if items.is_empty() {
                    println!("No runtime stream items found.");
                } else {
                    println!(
                        "{:<8}  {:<20}  {:<18}  {:<10}  SUMMARY",
                        "SEQ", "TIMESTAMP", "KIND", "STREAM"
                    );
                    println!("{}", "-".repeat(110));
                    for item in items {
                        println!(
                            "{:<8}  {:<20}  {:<18}  {:<10}  {}",
                            item.sequence,
                            item.timestamp.format("%Y-%m-%d %H:%M:%S"),
                            item.kind,
                            item.stream.as_deref().unwrap_or("-"),
                            item.title
                                .or(item.text)
                                .unwrap_or_else(|| item.event_id.clone())
                        );
                    }
                }
                ExitCode::Success
            }
            _ => output(&items, format)
                .map(|()| ExitCode::Success)
                .unwrap_or(ExitCode::Error),
        },
        Err(e) => output_error(&e, format),
    }
}
