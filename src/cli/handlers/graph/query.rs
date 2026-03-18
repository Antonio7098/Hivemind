use super::view::print_graph_query_result;
use super::super::common::get_graph_service;
use crate::cli::commands::GraphQueryCommands;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::graph_query::GraphQueryRequest;

pub(super) fn handle_graph_query(cmd: GraphQueryCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_graph_service(format) else {
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

    match service.graph_query_execute(&project, &request, "cli_graph_query") {
        Ok(result) => {
            print_graph_query_result(&result, format);
            ExitCode::Success
        }
        Err(e) => output_error(&e, format),
    }
}
