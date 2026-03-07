use super::*;

pub(crate) enum SelectedRuntimeAdapter {
    OpenCode(crate::adapters::opencode::OpenCodeAdapter),
    Codex(CodexAdapter),
    ClaudeCode(ClaudeCodeAdapter),
    Kilo(KiloAdapter),
    Native(NativeRuntimeAdapter),
}
#[derive(Debug, Clone)]
pub(crate) struct ClassifiedRuntimeError {
    pub(crate) code: String,
    pub(crate) category: String,
    pub(crate) message: String,
    pub(crate) recoverable: bool,
    pub(crate) retryable: bool,
    pub(crate) rate_limited: bool,
}
impl SelectedRuntimeAdapter {
    pub(crate) fn initialize(&mut self) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.initialize(),
            Self::Codex(a) => a.initialize(),
            Self::ClaudeCode(a) => a.initialize(),
            Self::Kilo(a) => a.initialize(),
            Self::Native(a) => a.initialize(),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        task_id: Uuid,
        worktree: &std::path::Path,
    ) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.prepare(task_id, worktree),
            Self::Codex(a) => a.prepare(task_id, worktree),
            Self::ClaudeCode(a) => a.prepare(task_id, worktree),
            Self::Kilo(a) => a.prepare(task_id, worktree),
            Self::Native(a) => a.prepare(task_id, worktree),
        }
    }

    pub(crate) fn execute(
        &mut self,
        input: ExecutionInput,
    ) -> std::result::Result<crate::adapters::runtime::ExecutionReport, RuntimeError> {
        match self {
            Self::OpenCode(a) => a.execute(input),
            Self::Codex(a) => a.execute(input),
            Self::ClaudeCode(a) => a.execute(input),
            Self::Kilo(a) => a.execute(input),
            Self::Native(a) => a.execute(input),
        }
    }

    pub(crate) fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        on_event: F,
    ) -> std::result::Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        match self {
            Self::OpenCode(a) => a.execute_interactive(input, on_event),
            Self::Codex(a) => a.execute_interactive(input, on_event),
            Self::ClaudeCode(a) => a.execute_interactive(input, on_event),
            Self::Kilo(a) => a.execute_interactive(input, on_event),
            Self::Native(a) => a.execute_interactive(input, on_event),
        }
    }
}
