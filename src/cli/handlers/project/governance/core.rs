use super::*;

pub(super) fn handle_project_governance_core(
    service: &GovernanceService,
    cmd: ProjectGovernanceCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceCommands::Init(args) => {
            let project = match support::resolve_required_selector(
                args.project.as_deref(),
                args.project_flag.as_deref(),
                "--project",
                "project",
                "cli:project:governance:init",
            ) {
                Ok(project) => project,
                Err(e) => return output_error(&e, format),
            };
            match service.project_governance_init(&project) {
                Ok(result) => {
                    render::print_project_governance_init(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Migrate(args) => {
            match service.project_governance_migrate(&args.project) {
                Ok(result) => {
                    render::print_project_governance_migrate(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Inspect(args) => {
            match service.project_governance_inspect(&args.project) {
                Ok(result) => {
                    render::print_project_governance_inspect(&result, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Diagnose(args) => {
            match service.project_governance_diagnose(&args.project) {
                Ok(result) => {
                    print_structured(&result, format, "project governance diagnostics");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectGovernanceCommands::Replay(args) => {
            match service.project_governance_replay(&args.project, args.verify) {
                Ok(result) => {
                    print_structured(&result, format, "project governance replay result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        _ => unreachable!("handled in caller"),
    }
}
