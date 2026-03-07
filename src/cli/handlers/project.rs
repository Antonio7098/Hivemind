//! Project command handlers.

use crate::cli::commands::{
    ProjectCommands, ProjectGovernanceAttachmentCommands, ProjectGovernanceCommands,
    ProjectGovernanceDocumentCommands, ProjectGovernanceNotepadCommands,
    ProjectGovernanceRepairCommands, ProjectGovernanceSnapshotCommands,
};
use crate::cli::handlers::common::{get_registry, parse_runtime_role, print_structured};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError};
use crate::core::registry::{
    ProjectGovernanceInitResult, ProjectGovernanceInspectResult, ProjectGovernanceMigrateResult,
    Registry,
};
use crate::core::scope::RepoAccessMode;
use crate::core::state::Project;
use uuid::Uuid;

mod governance;
mod render;
mod support;

pub fn handle_project(cmd: ProjectCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ProjectCommands::Create(args) => {
            match registry.create_project(&args.name, args.description.as_deref()) {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::List => match registry.list_projects() {
            Ok(projects) => {
                render::print_projects(&projects, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Inspect(args) => match registry.get_project(&args.project) {
            Ok(project) => {
                render::print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Update(args) => match registry.update_project(
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
        ProjectCommands::RuntimeSet(args) => match registry.project_runtime_set_role(
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
            match registry.attach_repo(&args.project, &args.path, args.name.as_deref(), access_mode)
            {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::DetachRepo(args) => {
            match registry.detach_repo(&args.project, &args.repo_name) {
                Ok(project) => {
                    render::print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::Delete(args) => match registry.delete_project(&args.project) {
            Ok(project_id) => {
                render::print_project_id(project_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Governance(cmd) => {
            governance::handle_project_governance(&registry, cmd, format)
        }
    }
}
