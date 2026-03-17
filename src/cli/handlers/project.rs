//! Project command handlers.

use crate::app::{GovernanceService, ProjectService};
use crate::cli::commands::{
    ProjectCommands, ProjectGovernanceAttachmentCommands, ProjectGovernanceCommands,
    ProjectGovernanceDocumentCommands, ProjectGovernanceNotepadCommands,
    ProjectGovernanceRepairCommands, ProjectGovernanceSnapshotCommands,
};
use crate::cli::handlers::common::{
    get_governance_service, get_project_service, parse_runtime_role, print_structured,
};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError};
use crate::core::registry::{
    ProjectGovernanceInitResult, ProjectGovernanceInspectResult, ProjectGovernanceMigrateResult,
};
use crate::core::scope::RepoAccessMode;
use crate::core::state::Project;
use uuid::Uuid;

mod governance;
mod render;
mod support;

pub fn handle_project(cmd: ProjectCommands, format: OutputFormat) -> ExitCode {
    let Some(project_service) = get_project_service(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ProjectCommands::Create(args) => {
            match project_service.create_project(&args.name, args.description.as_deref()) {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::List => match project_service.list_projects() {
            Ok(projects) => {
                render::print_projects(&projects, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Inspect(args) => match project_service.get_project(&args.project) {
            Ok(project) => {
                render::print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Update(args) => match project_service.update_project(
            &args.project,
            args.name.as_deref(),
            args.description.as_deref(),
        ) {
            Ok(project) => {
                render::print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::RuntimeSet(args) => match project_service.project_runtime_set_role(
            &args.project,
            parse_runtime_role(args.role),
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
            args.max_parallel_tasks,
        ) {
            Ok(project) => {
                render::print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::AttachRepo(args) => {
            let access_mode = match support::parse_repo_access_mode(&args.access, format) {
                Ok(mode) => mode,
                Err(code) => return code,
            };
            match project_service.attach_repo(
                &args.project,
                &args.path,
                args.name.as_deref(),
                access_mode,
            ) {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::DetachRepo(args) => {
            match project_service.detach_repo(&args.project, &args.repo_name) {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::Delete(args) => match project_service.delete_project(&args.project) {
            Ok(project_id) => {
                render::print_project_id(project_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Governance(cmd) => handle_project_governance(cmd, format),
    }
}

fn handle_project_governance(cmd: ProjectGovernanceCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_governance_service(format) else {
        return ExitCode::Error;
    };
    governance::handle_project_governance(&service, cmd, format)
}
