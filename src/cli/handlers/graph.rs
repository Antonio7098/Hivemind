//! Graph command handlers.

use crate::cli::commands::{GraphCommands, GraphQueryCommands, GraphSnapshotCommands};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::graph_query::{GraphQueryRequest, GraphQueryResult};
use crate::core::registry::Registry;
use uuid::Uuid;

fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

fn print_graph_id(graph_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"graph_id": graph_id}));
        }
        OutputFormat::Table => {
            println!("Graph ID: {graph_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"graph_id": graph_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

fn print_graphs(graphs: &[crate::core::graph::TaskGraph], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if graphs.is_empty() {
                println!("No graphs found.");
                return;
            }
            println!("{:<36}  {:<36}  {:<10}  NAME", "ID", "PROJECT", "STATE");
            println!("{}", "-".repeat(110));
            for g in graphs {
                println!(
                    "{:<36}  {:<36}  {:<10}  {}",
                    g.id,
                    g.project_id,
                    format!("{:?}", g.state).to_lowercase(),
                    g.name
                );
            }
        }
        _ => {
            if let Err(err) = output(graphs, format) {
                eprintln!("Failed to render graphs: {err}");
            }
        }
    }
}

fn print_graph_query_result(result: &GraphQueryResult, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("Query kind:           {}", result.query_kind);
            println!("Fingerprint:          {}", result.canonical_fingerprint);
            println!("Max results:          {}", result.max_results);
            println!("Truncated:            {}", result.truncated);
            println!("Visited nodes:        {}", result.cost.visited_nodes);
            println!("Visited edges:        {}", result.cost.visited_edges);
            println!("Result nodes:         {}", result.nodes.len());
            println!("Result edges:         {}", result.edges.len());
            if !result.nodes.is_empty() {
                println!("\nNodes:");
                println!(
                    "{:<40}  {:<10}  {:<14}  {:<24}  PATH",
                    "NODE ID", "REPO", "CLASS", "PARTITION"
                );
                println!("{}", "-".repeat(124));
                for node in &result.nodes {
                    println!(
                        "{:<40}  {:<10}  {:<14}  {:<24}  {}",
                        node.node_id,
                        node.repo_name,
                        node.node_class,
                        node.partition.as_deref().unwrap_or("-"),
                        node.path.as_deref().unwrap_or("-")
                    );
                }
            }
            if !result.edges.is_empty() {
                println!("\nEdges:");
                println!("{:<40}  {:<40}  TYPE", "SOURCE", "TARGET");
                println!("{}", "-".repeat(102));
                for edge in &result.edges {
                    println!(
                        "{:<40}  {:<40}  {}",
                        edge.source, edge.target, edge.edge_type
                    );
                }
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render graph query result: {err}");
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn handle_graph(cmd: GraphCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

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

            match registry.create_graph(&args.project, &args.name, &task_ids) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::Query(cmd) => handle_graph_query(cmd, format),
        GraphCommands::Snapshot(cmd) => handle_graph_snapshot(cmd, format),
        GraphCommands::AddDependency(args) => {
            match registry.add_graph_dependency(&args.graph_id, &args.from_task, &args.to_task) {
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

            match registry.add_graph_task_check(&args.graph_id, &args.task_id, check) {
                Ok(graph) => {
                    print_graph_id(graph.id, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GraphCommands::Validate(args) => match registry.validate_graph(&args.graph_id) {
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
        GraphCommands::List(args) => match registry.list_graphs(args.project.as_deref()) {
            Ok(graphs) => {
                print_graphs(&graphs, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        GraphCommands::Delete(args) => match registry.delete_graph(&args.graph_id) {
            Ok(graph_id) => {
                print_graph_id(graph_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
    }
}

pub fn handle_graph_snapshot(cmd: GraphSnapshotCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };
    match cmd {
        GraphSnapshotCommands::Refresh(args) => {
            match registry.graph_snapshot_refresh(&args.project, "manual_refresh") {
                Ok(result) => {
                    if format == OutputFormat::Table {
                        println!("Project:              {}", result.project_id);
                        println!("Snapshot path:        {}", result.path);
                        println!("Trigger:              {}", result.trigger);
                        println!("Repository count:     {}", result.repository_count);
                        println!("UCP profile:          {}", result.profile_version);
                        println!("UCP engine:           {}", result.ucp_engine_version);
                        println!("Fingerprint:          {}", result.canonical_fingerprint);
                        println!("Artifact revision:    {}", result.revision);
                    } else if let Err(err) = output(&result, format) {
                        eprintln!("Failed to render graph snapshot refresh result: {err}");
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

pub fn handle_graph_query(cmd: GraphQueryCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };
    let (project, request) = match cmd {
        GraphQueryCommands::Neighbors(args) => (
            args.project,
            GraphQueryRequest::Neighbors {
                node: args.node,
                edge_types: args.edge_types,
                max_results: Some(args.max_results),
            },
        ),
        GraphQueryCommands::Dependents(args) => (
            args.project,
            GraphQueryRequest::Dependents {
                node: args.node,
                edge_types: args.edge_types,
                max_results: Some(args.max_results),
            },
        ),
        GraphQueryCommands::Subgraph(args) => (
            args.project,
            GraphQueryRequest::Subgraph {
                seed: args.seed,
                depth: args.depth,
                edge_types: args.edge_types,
                max_results: Some(args.max_results),
            },
        ),
        GraphQueryCommands::Filter(args) => (
            args.project,
            GraphQueryRequest::Filter {
                node_type: args.node_type,
                path_prefix: args.path,
                partition: args.partition,
                max_results: Some(args.max_results),
            },
        ),
    };

    match registry.graph_query_execute(&project, &request, "cli_graph_query") {
        Ok(result) => {
            print_graph_query_result(&result, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
