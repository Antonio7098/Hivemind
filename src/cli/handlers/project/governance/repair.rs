use super::*;

pub(super) fn handle_repair_commands(
    service: &GovernanceService,
    cmd: ProjectGovernanceRepairCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceRepairCommands::Detect(args) => {
            match service.project_governance_repair_detect(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "governance repair detect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceRepairCommands::Preview(args) => {
            match service
                .project_governance_repair_preview(&args.project, args.snapshot_id.as_deref())
            {
                Ok(result) => {
                    print_structured(&result, format, "governance repair preview result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceRepairCommands::Apply(args) => {
            match service.project_governance_repair_apply(
                &args.project,
                args.snapshot_id.as_deref(),
                args.confirm,
            ) {
                Ok(result) => {
                    print_structured(&result, format, "governance repair apply result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
