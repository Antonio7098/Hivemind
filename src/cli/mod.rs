//! CLI commands and argument parsing.
//!
//! This module provides the command-line interface for Hivemind, built on
//! [`clap`](https://docs.rs/clap). The CLI is the primary interface for
//! interacting with Hivemind's orchestration capabilities.
//!
//! # Commands
//!
//! The CLI provides commands for:
//!
//! - **Project management**: `project create`, `project list`, `project show`
//! - **Task management**: `task create`, `task list`, `task show`
//! - **Graph/Flow execution**: `graph create`, `flow run`, `flow status`
//! - **Event inspection**: `events list`, `events show`
//! - **Server mode**: `serve` for HTTP API
//! - **Version info**: `version`
//!
//! # Output Formats
//!
//! Commands support multiple output formats via the `-f`/`--format` flag:
//!
//! - `text` - Human-readable table format (default)
//! - `json` - Machine-readable JSON
//!
//! # Example
//!
//! ```bash,no_run
//! # Create a project
//! hivemind project create my-project --repo /path/to/repo
//!
//! # List projects in JSON format
//! hivemind project list -f json
//!
//! # Start the HTTP server
//! hivemind serve --port 8787
//! ```
//!
//! # Modules
//!
//! - [`commands`] - Command implementations
//! - [`output`] - Output formatting and table rendering

pub mod commands;
pub mod output;
