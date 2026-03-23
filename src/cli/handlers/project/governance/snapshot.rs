use super::*;

pub(super) fn handle_snapshot_commands(
    service: &GovernanceService,
    cmd: ProjectGovernanceSnapshotCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceSnapshotCommands::Create(args) => {
            match service.project_governance_snapshot_create(&args.project, args.interval_minutes) {
                Ok(result) => {
                    print_structured(&result, format, "governance snapshot create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceSnapshotCommands::List(args) => {
            match service.project_governance_snapshot_list(&args.project, args.limit) {
                Ok(result) => {
                    print_structured(&result, format, "governance snapshot list result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceSnapshotCommands::Restore(args) => {
            match service.project_governance_snapshot_restore(
                &args.project,
                &args.snapshot_id,
                args.confirm,
            ) {
                Ok(result) => {
                    print_structured(&result, format, "governance snapshot restore result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
