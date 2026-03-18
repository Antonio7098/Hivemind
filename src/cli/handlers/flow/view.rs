use crate::cli::output::{output, OutputFormat};

pub(super) fn print_flows(flows: &[crate::core::flow::TaskFlow], format: OutputFormat) {
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
