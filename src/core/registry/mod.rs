//! Project registry for managing projects via events.
//!
//! The registry derives project state from events and provides
//! operations that emit new events.
//!
//! This module is the public facade for the registry functionality.
//! Implementation details are split into focused submodules.
#![allow(clippy::module_inception, unused_imports)]

pub mod context;
pub mod events;
pub mod flow;
pub mod governance;
pub mod graph;
pub mod registry;
pub mod runtime;
pub mod shared_prelude;
pub mod shared_types;
pub mod tasks;
pub mod templates;
pub mod types;
pub mod workflow;
pub mod worktree;

pub use context::*;
pub use events::*;
pub use flow::*;
pub use governance::*;
pub use graph::*;
pub use registry::{Registry, RegistryConfig};
pub use runtime::*;
pub use tasks::*;
pub use templates::*;
pub use types::*;
pub use workflow::*;
pub use worktree::*;

pub const GOVERNANCE_SCHEMA_VERSION: &str = "governance.v1";
pub const GOVERNANCE_PROJECTION_VERSION: u32 = 1;
pub const GOVERNANCE_FROM_LAYOUT: &str = "repo_local_hivemind_v1";
pub const GOVERNANCE_TO_LAYOUT: &str = "global_governance_v1";
pub const GOVERNANCE_EXPORT_IMPORT_BOUNDARY: &str = "Manual export/import only (not auto-enabled).";
pub const CONSTITUTION_SCHEMA_VERSION: &str = "constitution.v1";
pub const CONSTITUTION_VERSION: u32 = 1;
pub const GRAPH_SNAPSHOT_SCHEMA_VERSION: &str = "graph_snapshot.v1";
pub const GRAPH_SNAPSHOT_VERSION: u32 = 1;
pub const GOVERNANCE_RECOVERY_SNAPSHOT_SCHEMA_VERSION: &str = "governance_recovery_snapshot.v1";
pub const ATTEMPT_CONTEXT_SCHEMA_VERSION: &str = "attempt_context.v2";
pub const ATTEMPT_CONTEXT_VERSION: u32 = 2;
pub const ATTEMPT_CONTEXT_SECTION_BUDGET_BYTES: usize = 6_000;
pub const ATTEMPT_CONTEXT_TOTAL_BUDGET_BYTES: usize = 24_000;
pub const ATTEMPT_CONTEXT_MAX_EXPAND_DEPTH: usize = 2;
pub const ATTEMPT_CONTEXT_TRUNCATION_POLICY: &str = "ordered_section_then_total_budget";
