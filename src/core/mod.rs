//! Core domain types: events, state, errors, graphs, and flows.
//!
//! This module contains the heart of Hivemind's domain model. All state
//! is derived from immutable events, ensuring determinism, idempotency,
//! and complete observability.
//!
//! # Architecture
//!
//! ```text
//! Events (immutable) → State (derived) → Actions (emit new events)
//! ```
//!
//! # Key Concepts
//!
//! ## Events
//!
//! Events are the single source of truth. They are:
//! - **Immutable**: Once created, never modified
//! - **Append-only**: New events are added, never removed
//! - **Ordered**: Each event has a unique, ordered ID
//!
//! See [`events`] for event types and [`EventId`](events::EventId).
//!
//! ## State
//!
//! State is derived by replaying events. This means:
//! - State can be rebuilt from scratch at any time
//! - No hidden state or side effects
//! - Complete audit trail
//!
//! See [`state`] for state types like [`Project`](state::Project) and [`Task`](state::Task).
//!
//! ## Graphs and Flows
//!
//! - [`TaskGraph`](graph::TaskGraph): Static, immutable DAG representing planned intent
//! - [`TaskFlow`](flow::TaskFlow): Runtime instance of a `TaskGraph` with execution state
//!
//! Multiple flows can exist for the same graph (e.g., retries, parallel attempts).
//!
//! ## Scope
//!
//! Execution scope defines what an agent can access:
//! - Which repositories
//! - What file patterns (include/exclude)
//! - What operations (read, write, delete)
//!
//! See [`scope`] and [`enforcement`] for scope definition and enforcement.
//!
//! ## Errors
//!
//! All errors are structured with:
//! - Category (system, runtime, agent, scope, verification, git, user, policy)
//! - Code (unique within category)
//! - Message (human-readable)
//! - Origin (component that produced the error)
//! - Recovery hint (when applicable)
//!
//! See [`error`] for [`HivemindError`](error::HivemindError) and [`Result`](error::Result).
//!
//! # Modules
//!
//! - [`events`] - Event definitions and types
//! - [`state`] - Derived state types (Project, Task, Attempt, etc.)
//! - [`graph`] - `TaskGraph`: static planning DAG
//! - [`flow`] - `TaskFlow`: runtime execution state
//! - [`error`] - Structured error types
//! - [`scope`] - Execution scope definitions
//! - [`enforcement`] - Scope enforcement and verification
//! - [`verification`] - Check configurations and results
//! - [`diff`] - Diff types and utilities
//! - [`registry`] - Project registry for event-based state management
//! - [`scheduler`] - Task scheduling and execution
//! - [`worktree`] - Git worktree management
//! - [`runtime_event_projection`] - Runtime event observation projection

pub mod context_window;
pub mod diff;
pub mod enforcement;
pub mod error;
pub mod events;
pub mod flow;
pub mod graph;
pub mod registry;
pub mod runtime_event_projection;
pub mod scheduler;
pub mod scope;
pub mod skill_registry;
pub mod state;
pub mod verification;
pub mod worktree;
