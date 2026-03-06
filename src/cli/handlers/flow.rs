//! Flow command handlers.

use crate::cli::commands::FlowCommands;
use crate::cli::commands::{MergeExecuteModeArg, RunModeArg, RuntimeRoleArg};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::events::RuntimeRole;
use crate::core::flow::RunMode;
use crate::core::registry::{MergeExecuteMode, Registry};
use uuid::Uuid;

fn parse_runtime_role(role: RuntimeRoleArg) -> RuntimeRole {
    match role {
        RuntimeRoleArg::Worker => RuntimeRole::Worker,
        RuntimeRoleArg::Validator => RuntimeRole::Validator,
    }
}

fn parse_run_mode(mode: RunModeArg) -> RunMode {
    match mode {
        RunModeArg::Auto => RunMode::Auto,
        RunModeArg::Manual => RunMode::Manual,
    }
}

#[allow(dead_code)]
fn parse_merge_execute_mode(mode: MergeExecuteModeArg) -> MergeExecuteMode {
    match mode {
        MergeExecuteModeArg::Local => MergeExecuteMode::Local,
        MergeExecuteModeArg::Pr => MergeExecuteMode::Pr,
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

fn print_flow_id(flow_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"flow_id": flow_id}));
        }
        OutputFormat::Table => {
            println!("Flow ID: {flow_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"flow_id": flow_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn print_flows(flows: &[crate::core::flow::TaskFlow], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if flows.is_empty() {
                println!("No flows found.");
                return;
            }
            println!(
                "{:<36}  {:<36}  {:<10}  {:<6}  GRAPH",
                "ID", "PROJECT", "STATE", "MODE"
            );
            println!("{}", "-".repeat(110));
            for f in flows {
                println!(
                    "{:<36}  {:<36}  {:<10}  {:<6}  {}",
                    f.id,
                    f.project_id,
                    format!("{:?}", f.state).to_lowercase(),
                    format!("{:?}", f.run_mode).to_lowercase(),
                    f.graph_id
                );
            }
        }
        _ => {
            if let Err(err) = output(flows, format) {
                eprintln!("Failed to render flows: {err}");
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn handle_flow(cmd: FlowCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        FlowCommands::Create(args) => {
            match registry.create_flow(&args.graph_id, args.name.as_deref()) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::List(args) => match registry.list_flows(args.project.as_deref()) {
            Ok(flows) => {
                print_flows(&flows, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Start(args) => match registry.start_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Tick(args) => {
            match registry.tick_flow(&args.flow_id, args.interactive, args.max_parallel) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Pause(args) => {
            let _ = args.wait;
            match registry.pause_flow(&args.flow_id) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Resume(args) => match registry.resume_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Abort(args) => {
            match registry.abort_flow(&args.flow_id, args.reason.as_deref(), args.force) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Restart(args) => {
            match registry.restart_flow(&args.flow_id, args.name.as_deref(), args.start) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Status(args) => match registry.get_flow(&args.flow_id) {
            Ok(flow) => match format {
                OutputFormat::Json | OutputFormat::Yaml => {
                    if let Err(err) = output(&flow, format) {
                        eprintln!("Failed to render flow status: {err}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    println!("ID:      {}", flow.id);
                    println!("Graph:   {}", flow.graph_id);
                    println!("Project: {}", flow.project_id);
                    println!("State:   {:?}", flow.state);
                    println!("RunMode: {:?}", flow.run_mode);
                    if !flow.depends_on_flows.is_empty() {
                        let mut deps: Vec<_> = flow.depends_on_flows.iter().copied().collect();
                        deps.sort();
                        println!(
                            "FlowDeps: {}",
                            deps.iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    let mut counts = std::collections::HashMap::new();
                    for exec in flow.task_executions.values() {
                        *counts.entry(exec.state).or_insert(0usize) += 1;
                    }
                    println!("Tasks:");
                    let mut keys: Vec<_> = counts.keys().copied().collect();
                    keys.sort_by_key(|k| format!("{k:?}"));
                    for k in keys {
                        println!("  - {:?}: {}", k, counts[&k]);
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        FlowCommands::SetRunMode(args) => {
            let mode = parse_run_mode(args.mode);
            match registry.flow_set_run_mode(&args.flow_id, mode) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::AddDependency(args) => {
            match registry.flow_add_dependency(&args.flow_id, &args.depends_on_flow_id) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::RuntimeSet(args) => {
            let role = parse_runtime_role(args.role);
            let result = if args.clear {
                registry.flow_runtime_clear(&args.flow_id, role)
            } else {
                registry.flow_runtime_set(
                    &args.flow_id,
                    role,
                    &args.adapter,
                    &args.binary_path,
                    args.model,
                    &args.args,
                    &args.env,
                    args.timeout_ms,
                    args.max_parallel_tasks,
                )
            };
            match result {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Delete(args) => match registry.delete_flow(&args.flow_id) {
            Ok(flow_id) => {
                print_flow_id(flow_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
