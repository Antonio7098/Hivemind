use super::super::common::{parse_run_mode, parse_runtime_role, print_flow_id};
use super::view::print_flows;
use crate::app::FlowService;
use crate::cli::commands::FlowCommands;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;

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
        _ => unreachable!("handled in caller"),
    }
}
