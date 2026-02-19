//! # Hivemind
//!
//! A **local-first, observable orchestration system** for agentic software development.
//!
//! Hivemind coordinates planning, execution, verification, and integration for autonomous
//! agents working on real codebases. It treats agentic work as engineering work, deserving
//! of explicit planning, deterministic structure, observable execution, auditable outcomes,
//! and human authority at critical boundaries.
//!
//! ## Core Philosophy
//!
//! > Agents execute. Systems govern. Humans decide.
//!
//! ## Key Features
//!
//! - **Event-sourced architecture**: All state derived from immutable, append-only events
//! - **Deterministic task graphs**: Static DAGs representing planned intent
//! - **Scoped execution**: Fine-grained access control for agent operations
//! - **Runtime adapters**: Pluggable execution backends (`OpenCode`, Claude Code, Codex, Kilo)
//! - **Verification**: Automated checks with configurable retry policies
//! - **Full observability**: Complete diff, commit, and event history
//! - **Local-first**: Everything runs locally, no cloud dependencies
//!
//! ## Architecture
//!
//! The crate is organized into several modules:
//!
//! - [`core`] - Domain types: events, state, graphs, flows, errors, and scope enforcement
//! - [`adapters`] - Runtime adapters for execution backends (`OpenCode`, Claude Code, etc.)
//! - [`native`] - Native runtime contracts and deterministic harness
//! - [`storage`] - Event persistence layer (SQLite-backed event store)
//! - [`cli`] - Command-line interface commands and output formatting
//! - [`server`] - HTTP API server for UI integration
//!
//! ## Quick Start
//!
//! ```bash,no_run
//! # Install via cargo
//! cargo install hivemind-core
//!
//! # Verify installation
//! hivemind --version
//! ```
//!
//! ## Example: Creating a Project
//!
//! ```no_run
//! use hivemind::core::registry::Registry;
//!
//! let registry = Registry::open()?;
//! let project = registry.create_project("my-project", "/path/to/repo")?;
//! println!("Created project: {}", project.id);
//! # Ok::<(), hivemind::core::error::HivemindError>(())
//! ```
//!
//! ## Example: Defining a Task Graph
//!
//! ```no_run
//! use hivemind::core::graph::{TaskGraph, GraphTask, SuccessCriteria};
//!
//! let task = GraphTask::new("Implement feature X", SuccessCriteria::new("Feature works correctly"));
//! let graph = TaskGraph::new("feature-x").with_task(task);
//! ```
//!
//! ## Event-Sourced State
//!
//! All state in Hivemind is derived by replaying events. This ensures:
//!
//! - **Determinism**: Same events always produce the same state
//! - **Idempotency**: Operations can be safely retried
//! - **Complete observability**: Full audit trail of all changes
//! - **Time travel**: Query state at any point in history
//!
//! ## Documentation
//!
//! - **Canonical docs**: <https://hivemind-landing.netlify.app/>
//! - **Quickstart guide**: <https://hivemind-landing.netlify.app/docs/overview/quickstart>
//! - **Repository**: <https://github.com/Antonio7098/hivemind>
//!
//! ## License
//!
//! MIT

pub mod adapters;
pub mod cli;
pub mod core;
pub mod native;
pub mod server;
pub mod storage;
