use crate::cli::output::{output, OutputFormat};
use crate::core::graph_query::GraphQueryResult;
use uuid::Uuid;

pub(super) fn print_graph_id(graph_id: Uuid, format: OutputFormat) {
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

pub(super) fn print_graphs(graphs: &[crate::core::graph::TaskGraph], format: OutputFormat) {
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

pub(super) fn print_graph_query_result(result: &GraphQueryResult, format: OutputFormat) {
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
