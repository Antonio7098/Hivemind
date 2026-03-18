use super::*;

pub(super) mod attachment;
pub(super) mod core;
pub(super) mod document;
pub(super) mod notepad;
pub(super) mod repair;
pub(super) mod snapshot;

#[allow(clippy::too_many_lines)]
pub(super) fn handle_project_governance(
    service: &GovernanceService,
    cmd: ProjectGovernanceCommands,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        ProjectGovernanceCommands::Init(_)
        | ProjectGovernanceCommands::Migrate(_)
        | ProjectGovernanceCommands::Inspect(_)
        | ProjectGovernanceCommands::Diagnose(_)
        | ProjectGovernanceCommands::Replay(_) => core::handle_project_governance_core(service, cmd, format),
        ProjectGovernanceCommands::Snapshot(subcmd) => {
            snapshot::handle_snapshot_commands(service, subcmd, format)
        }
        ProjectGovernanceCommands::Repair(subcmd) => {
            repair::handle_repair_commands(service, subcmd, format)
        }
        ProjectGovernanceCommands::Document(subcmd) => {
            document::handle_document_commands(service, subcmd, format)
        }
        ProjectGovernanceCommands::Attachment(subcmd) => {
            attachment::handle_attachment_commands(service, subcmd, format)
        }
        ProjectGovernanceCommands::Notepad(subcmd) => {
            notepad::handle_notepad_commands(service, subcmd, format)
        }
    }
}
