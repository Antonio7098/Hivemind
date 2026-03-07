//! Task command handlers.

use crate::cli::commands::{
    TaskAbortArgs, TaskCloseArgs, TaskCommands, TaskCompleteArgs, TaskCreateArgs, TaskInspectArgs,
    TaskListArgs, TaskRetryArgs, TaskStartArgs, TaskUpdateArgs,
};
use crate::cli::handlers::common::{
    get_registry, parse_run_mode, parse_runtime_role, print_flow_id,
};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::flow::RetryMode;
use crate::core::registry::Registry;
use crate::core::scope::Scope;
use crate::core::state::{Task, TaskState};
use uuid::Uuid;
mod render;
pub use render::*;
mod commands;
pub use commands::*;
mod dispatch;
pub use dispatch::*;
