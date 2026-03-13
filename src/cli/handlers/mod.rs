//! CLI command handlers.
//!
//! This module contains the handler functions for CLI commands,
//! split by feature area to keep main.rs focused.
#![allow(unused_imports, unused_variables, clippy::needless_pass_by_value)]

pub mod attempt;
pub mod checkpoint;
pub mod common;
pub mod events;
pub mod flow;
pub mod global;
pub mod governance;
pub mod graph;
pub mod merge;
pub mod project;
pub mod runtime;
pub mod serve;
pub mod task;
pub mod verify;
pub mod workflow;
pub mod worktree;

// Re-export common handler utilities
pub use attempt::*;
pub use checkpoint::*;
pub use events::*;
pub use flow::*;
pub use global::*;
pub use governance::*;
pub use graph::*;
pub use merge::*;
pub use project::*;
pub use runtime::*;
pub use serve::*;
pub use task::*;
pub use verify::*;
pub use workflow::*;
pub use worktree::*;
