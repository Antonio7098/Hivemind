use super::common::get_worktree_service;
use crate::cli::commands::WorktreeCommands;
use crate::cli::output::OutputFormat;
use crate::core::error::ExitCode;

pub(super) mod core;

pub fn handle_worktree(cmd: WorktreeCommands, format: OutputFormat) -> ExitCode {
    let Some(service) = get_worktree_service(format) else {
        return ExitCode::Error;
    };

    core::handle_worktree_core(&service, cmd, format)
}
