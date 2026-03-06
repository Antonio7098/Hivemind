//! Governance command handlers.

use crate::cli::commands::{
    ConstitutionCommands, ProjectGovernanceCommands, ProjectGovernanceDocumentCommands,
    ProjectGovernanceNotepadCommands, ProjectGovernanceRepairCommands,
    ProjectGovernanceSnapshotCommands,
};
use crate::cli::handlers::common::{get_registry, print_structured};
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError};
use std::fs;

fn read_constitution_payload(
    content: Option<&str>,
    from_file: Option<&str>,
    origin: &'static str,
) -> std::result::Result<Option<String>, HivemindError> {
    if let Some(raw) = content {
        return Ok(Some(raw.to_string()));
    }
    let Some(path) = from_file else {
        return Ok(None);
    };
    let payload = fs::read_to_string(path).map_err(|e| {
        HivemindError::user(
            "constitution_input_read_failed",
            format!("Failed to read constitution file '{path}': {e}"),
            origin,
        )
        .with_hint("Ensure --from-file points to a readable YAML file")
    })?;
    Ok(Some(payload))
}

pub fn handle_constitution(cmd: ConstitutionCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ConstitutionCommands::Init(args) => {
            let payload = match read_constitution_payload(
                args.content.as_deref(),
                args.from_file.as_deref(),
                "cli:constitution:init",
            ) {
                Ok(value) => value,
                Err(err) => return output_error(&err, format),
            };
            match registry.constitution_init(
                &args.project,
                payload.as_deref(),
                args.confirm,
                args.actor.as_deref(),
                args.intent.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "constitution init result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ConstitutionCommands::Show(args) => match registry.constitution_show(&args.project) {
            Ok(result) => {
                print_structured(&result, format, "constitution show result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ConstitutionCommands::Validate(args) => {
            match registry.constitution_validate(&args.project, None) {
                Ok(result) => {
                    print_structured(&result, format, "constitution validate result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ConstitutionCommands::Check(args) => match registry.constitution_check(&args.project) {
            Ok(result) => {
                print_structured(&result, format, "constitution check result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ConstitutionCommands::Update(args) => {
            let payload = match read_constitution_payload(
                args.content.as_deref(),
                args.from_file.as_deref(),
                "cli:constitution:update",
            ) {
                Ok(Some(value)) => value,
                Ok(None) => {
                    return output_error(
                        &HivemindError::user(
                            "constitution_content_missing",
                            "Constitution update requires --content or --from-file",
                            "cli:constitution:update",
                        )
                        .with_hint(
                            "Provide a YAML payload via --content '<yaml>' or --from-file <path>",
                        ),
                        format,
                    );
                }
                Err(err) => return output_error(&err, format),
            };

            match registry.constitution_update(
                &args.project,
                &payload,
                args.confirm,
                args.actor.as_deref(),
                args.intent.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "constitution update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

pub fn handle_governance(_cmd: ProjectGovernanceCommands, _format: OutputFormat) -> ExitCode {
    todo!("Move implementation from project.rs if governance subcommand is lifted")
}

pub fn handle_governance_document(
    _cmd: ProjectGovernanceDocumentCommands,
    _format: OutputFormat,
) -> ExitCode {
    todo!("Move implementation from project.rs if governance subcommand is lifted")
}

pub fn handle_governance_notepad(
    _cmd: ProjectGovernanceNotepadCommands,
    _format: OutputFormat,
) -> ExitCode {
    todo!("Move implementation from project.rs if governance subcommand is lifted")
}

pub fn handle_governance_repair(
    _cmd: ProjectGovernanceRepairCommands,
    _format: OutputFormat,
) -> ExitCode {
    todo!("Move implementation from project.rs if governance subcommand is lifted")
}

pub fn handle_governance_snapshot(
    _cmd: ProjectGovernanceSnapshotCommands,
    _format: OutputFormat,
) -> ExitCode {
    todo!("Move implementation from project.rs if governance subcommand is lifted")
}
