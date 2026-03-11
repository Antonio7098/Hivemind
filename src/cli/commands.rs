//! CLI command definitions.
//!
//! All features are accessible via CLI. The UI is a projection, not a controller.

use super::output::OutputFormat;
use clap::{Args, Parser, Subcommand};

mod graph_flow;
pub use graph_flow::*;

mod governance_global;
pub use governance_global::*;

mod task_runtime_event;
pub use task_runtime_event::*;

mod common;
#[cfg(test)]
mod tests;
pub use common::*;
mod checkpoint_worktree;
pub use checkpoint_worktree::*;
mod task_flow;
pub use task_flow::*;
mod project;
pub use project::*;
mod workflow;
pub use workflow::*;
mod verify_merge_attempt;
pub use verify_merge_attempt::*;
