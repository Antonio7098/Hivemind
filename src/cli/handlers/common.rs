//! Shared helpers for CLI command handlers.

use crate::cli::commands::{MergeExecuteModeArg, RunModeArg, RuntimeRoleArg};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::events::RuntimeRole;
use crate::core::flow::RunMode;
use crate::core::registry::{MergeExecuteMode, Registry};
use uuid::Uuid;

pub(crate) fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
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
