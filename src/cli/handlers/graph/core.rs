use super::view::{print_graph_id, print_graphs};
use crate::app::GraphService;
use crate::cli::commands::GraphCommands;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use uuid::Uuid;

pub(super) fn handle_graph_core(
    service: &GraphService,
    cmd: GraphCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GraphCommands::Create(args) => {
            let mut task_ids = Vec::new();
            for raw in &args.from_tasks {
                let Ok(id) = Uuid::parse_str(raw) else {
                    return output_error(
                        &crate::core::error::HivemindError::user(
                            "invalid_task_id",
                            format!("'{raw}' is not a valid task ID"),
                            "cli:graph:create",
                        ),
                        format,
                    );
                };
                task_ids.push(id);
            }

            match service.create_graph(&args.project, &args.name, &task_ids) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::AddDependency(args) => {
            match service.add_graph_dependency(&args.graph_id, &args.from_task, &args.to_task) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::AddCheck(args) => {
            let mut check = crate::core::verification::CheckConfig::new(
                args.name.clone(),
                args.command.clone(),
            );
            check.required = args.required;
            check.timeout_ms = args.timeout_ms;

            match service.add_graph_task_check(&args.graph_id, &args.task_id, check) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::Validate(args) => match service.validate_graph(&args.graph_id) {
            Ok(result) => match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&result) {
                        println!("{json}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&result) {
                        print!("{yaml}");
                    }
                    ExitCode::Success
                }
                OutputFormat::Table => {
                    if result.valid {
                        println!("valid");
                    } else {
                        println!("invalid");
                        for issue in result.issues {
                            println!("- {issue}");
                        }
                    }
                    ExitCode::Success
                }
            },
            Err(e) => output_error(&e, format),
        },
        GraphCommands::List(args) => match service.list_graphs(args.project.as_deref()) {
            Ok(graphs) => {
                print_graphs(&graphs, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GraphCommands::Delete(args) => match service.delete_graph(&args.graph_id) {
            Ok(graph_id) => {
                print_graph_id(graph_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        _ => unreachable!("Handled in caller"),
    }
}
