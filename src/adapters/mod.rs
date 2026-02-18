//! Runtime adapters for agent execution.
//!
//! This module provides the adapter interface and implementations for various
//! AI coding agent runtimes. Adapters are the bridge between Hivemind's
//! orchestration layer and actual agent execution.
//!
//! # Supported Runtimes
//!
//! | Adapter | Binary | OpenCode Compatible |
//! |---------|--------|---------------------|
//! | `opencode` | `opencode` | Yes |
//! | `codex` | `codex` | No |
//! | `claude-code` | `claude` | No |
//! | `kilo` | `kilo` | Yes |
//!
//! # Architecture
//!
//! Each adapter implements the [`runtime::RuntimeAdapter`] trait, providing:
//!
//! - Task execution with configurable timeouts
//! - Output capture (stdout, stderr, file changes)
//! - Interactive event streaming
//! - Error handling and classification
//!
//! # Example
//!
//! ```no_run
//! use hivemind::adapters::{runtime_descriptors, SUPPORTED_ADAPTERS};
//!
//! // List available adapters
//! for name in SUPPORTED_ADAPTERS {
//!     println!("Available adapter: {}", name);
//! }
//!
//! // Get adapter descriptors with binary paths
//! for desc in runtime_descriptors() {
//!     println!("{}: binary={}", desc.adapter_name, desc.default_binary);
//! }
//! ```
//!
//! # Modules
//!
//! - [`runtime`] - Core adapter trait and types
//! - [`opencode`] - OpenCode adapter (primary)
//! - [`claude_code`] - Claude Code adapter
//! - [`codex`] - Codex CLI adapter
//! - [`kilo`] - Kilo adapter

pub mod claude_code;
pub mod codex;
pub mod kilo;
pub mod opencode;
pub mod runtime;

/// Supported runtime adapter names.
pub const SUPPORTED_ADAPTERS: [&str; 4] = ["opencode", "codex", "claude-code", "kilo"];

/// Built-in runtime descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    pub adapter_name: &'static str,
    pub default_binary: &'static str,
    pub opencode_compatible: bool,
}

/// Returns descriptors for built-in runtime adapters.
#[must_use]
pub fn runtime_descriptors() -> [RuntimeDescriptor; 4] {
    [
        RuntimeDescriptor {
            adapter_name: "opencode",
            default_binary: "opencode",
            opencode_compatible: true,
        },
        RuntimeDescriptor {
            adapter_name: "codex",
            default_binary: "codex",
            opencode_compatible: false,
        },
        RuntimeDescriptor {
            adapter_name: "claude-code",
            default_binary: "claude",
            opencode_compatible: false,
        },
        RuntimeDescriptor {
            adapter_name: "kilo",
            default_binary: "kilo",
            opencode_compatible: true,
        },
    ]
}
