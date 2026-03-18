use super::super::common::get_graph_service;
use crate::cli::commands::GraphSnapshotCommands;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;

pub(super) fn handle_graph_snapshot(cmd: GraphSnapshotCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_graph_service(format) else {
        return ExitCode::Error;
    };
    match cmd {
        GraphSnapshotCommands::Refresh(args) => {
            match service.graph_snapshot_refresh(&args.project, "manual_refresh") {
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
