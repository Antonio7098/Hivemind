//! Shared helpers for CLI command handlers.

use crate::app::{
    AppContext, AttemptService, CheckpointService, EventService, FlowService, GovernanceService,
    GraphService, MergeService, ProjectService, RuntimeService, TaskService, VerificationService,
    WorktreeService,
};
use crate::cli::commands::{MergeExecuteModeArg, RunModeArg, RuntimeRoleArg};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::events::RuntimeRole;
use crate::core::flow::RunMode;
use crate::core::registry::MergeExecuteMode;
use uuid::Uuid;

pub(crate) fn app_context() -> AppContext {
    AppContext::default()
}

pub(crate) fn get_flow_service(format: OutputFormat) -> Option<FlowService> {
    match app_context().flow_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_graph_service(format: OutputFormat) -> Option<GraphService> {
    match app_context().graph_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_runtime_service(format: OutputFormat) -> Option<RuntimeService> {
    match app_context().runtime_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_project_service(format: OutputFormat) -> Option<ProjectService> {
    match app_context().project_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_governance_service(format: OutputFormat) -> Option<GovernanceService> {
    match app_context().governance_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_merge_service(format: OutputFormat) -> Option<MergeService> {
    match app_context().merge_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_verification_service(format: OutputFormat) -> Option<VerificationService> {
    match app_context().verification_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_checkpoint_service(format: OutputFormat) -> Option<CheckpointService> {
    match app_context().checkpoint_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_task_service(format: OutputFormat) -> Option<TaskService> {
    match app_context().task_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_attempt_service(format: OutputFormat) -> Option<AttemptService> {
    match app_context().attempt_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_event_service(format: OutputFormat) -> Option<EventService> {
    match app_context().event_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn get_worktree_service(format: OutputFormat) -> Option<WorktreeService> {
    match app_context().worktree_service() {
        Ok(service) => Some(service),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

pub(crate) fn parse_merge_execute_mode(mode: MergeExecuteModeArg) -> MergeExecuteMode {
    match mode {
        MergeExecuteModeArg::Local => MergeExecuteMode::Local,
        MergeExecuteModeArg::Pr => MergeExecuteMode::Pr,
    }
}

pub(crate) fn parse_runtime_role(role: RuntimeRoleArg) -> RuntimeRole {
    match role {
        RuntimeRoleArg::Worker => RuntimeRole::Worker,
        RuntimeRoleArg::Validator => RuntimeRole::Validator,
    }
}

pub(crate) fn parse_run_mode(mode: RunModeArg) -> RunMode {
    match mode {
        RunModeArg::Manual => RunMode::Manual,
        RunModeArg::Auto => RunMode::Auto,
    }
}

pub(crate) fn print_structured<T: serde::Serialize>(
    value: &T,
    format: OutputFormat,
    context: &str,
) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(value) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(value, format) {
                eprintln!("Failed to render {context}: {err}");
            }
        }
    }
}

pub(crate) fn print_flow_id(flow_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"flow_id": flow_id}));
        }
        OutputFormat::Table => {
            println!("Flow ID: {flow_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"flow_id": flow_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}
