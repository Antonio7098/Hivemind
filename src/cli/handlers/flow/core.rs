use super::super::common::{parse_run_mode, parse_runtime_role, print_flow_id};
use super::view::print_flows;
use crate::app::FlowService;
use crate::cli::commands::FlowCommands;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;

// ARCH_DEBT: legacy oversized function awaiting CLI handler refactor
#[allow(clippy::too_many_lines)]
pub(super) fn handle_flow_core(
    service: &FlowService,
    cmd: FlowCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        FlowCommands::Create(args) => {
            match service.create_flow(&args.graph_id, args.name.as_deref()) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::List(args) => match service.list_flows(args.project.as_deref()) {
            Ok(flows) => {
                print_flows(&flows, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Start(args) => match service.start_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Tick(args) => {
            match service.tick_flow(&args.flow_id, args.interactive, args.max_parallel) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Pause(args) => {
            let _ = args.wait;
            match service.pause_flow(&args.flow_id) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Resume(args) => match service.resume_flow(&args.flow_id) {
            Ok(flow) => {
                print_flow_id(flow.id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        FlowCommands::Abort(args) => {
            match service.abort_flow(&args.flow_id, args.reason.as_deref(), args.force) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Restart(args) => {
            match service.restart_flow(&args.flow_id, args.name.as_deref(), args.start) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::Status(args) => match service.get_flow(&args.flow_id) {
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
            match service.flow_set_run_mode(&args.flow_id, mode) {
                Ok(flow) => {
                    print_flow_id(flow.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        FlowCommands::AddDependency(args) => {
            match service.flow_add_dependency(&args.flow_id, &args.depends_on_flow_id) {
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
                service.flow_runtime_clear(&args.flow_id, role)
            } else {
                service.flow_runtime_set(
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
        FlowCommands::Delete(args) => match service.delete_flow(&args.flow_id) {
            Ok(flow_id) => {
                print_flow_id(flow_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}
