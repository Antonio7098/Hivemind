//! Runtime adapters for agent execution.
//!
//! This module provides the adapter interface and implementations for various
//! AI coding agent runtimes. Adapters are the bridge between Hivemind's
//! orchestration layer and actual agent execution.
//!
//! # Supported Runtimes
//!
//! | Adapter | Binary | `OpenCode` Compatible |
//! |---------|--------|---------------------|
//! | `opencode` | `opencode` | Yes |
//! | `codex` | `codex` | No |
//! | `claude-code` | `claude` | No |
//! | `kilo` | `kilo` | Yes |
//! | `native` | `builtin-native` | No |
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
//! - [`opencode`] - `OpenCode` adapter (primary)
//! - [`claude_code`] - Claude Code adapter
//! - [`codex`] - Codex CLI adapter
//! - [`kilo`] - Kilo adapter

pub mod claude_code;
pub mod codex;
mod json_output;
pub mod kilo;
pub mod opencode;
pub mod runtime;

/// Supported runtime adapter names.
pub const SUPPORTED_ADAPTERS: [&str; 5] = ["opencode", "codex", "claude-code", "kilo", "native"];

/// Built-in runtime descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDescriptor {
    pub adapter_name: &'static str,
    pub default_binary: &'static str,
    pub opencode_compatible: bool,
    pub requires_binary: bool,
    pub capabilities: &'static [&'static str],
    pub aliases: &'static [&'static str],
}

impl RuntimeDescriptor {
    fn matches(&self, name: &str) -> bool {
        let normalized = normalize_adapter_name(name);
        self.adapter_name == normalized
            || self
                .aliases
                .iter()
                .any(|alias| normalize_adapter_name(alias) == normalized)
    }
}

const RUNTIME_DESCRIPTORS: [RuntimeDescriptor; 5] = [
    RuntimeDescriptor {
        adapter_name: "opencode",
        default_binary: "opencode",
        opencode_compatible: true,
        requires_binary: true,
        capabilities: &[
            "external_cli",
            "opencode_family",
            "interactive_transport",
            "structured_json_output",
            "session_resume",
            "interrupt_runtime",
        ],
        aliases: &["opencode-cli"],
    },
    RuntimeDescriptor {
        adapter_name: "codex",
        default_binary: "codex",
        opencode_compatible: false,
        requires_binary: true,
        capabilities: &[
            "external_cli",
            "tool_events",
            "interactive_transport",
            "structured_json_output",
            "session_resume",
        ],
        aliases: &[],
    },
    RuntimeDescriptor {
        adapter_name: "claude-code",
        default_binary: "claude",
        opencode_compatible: false,
        requires_binary: true,
        capabilities: &["external_cli", "tool_events", "interactive_transport"],
        aliases: &["claude"],
    },
    RuntimeDescriptor {
        adapter_name: "kilo",
        default_binary: "kilo",
        opencode_compatible: true,
        requires_binary: true,
        capabilities: &[
            "external_cli",
            "opencode_family",
            "interactive_transport",
            "structured_json_output",
            "session_resume",
            "interrupt_runtime",
        ],
        aliases: &[],
    },
    RuntimeDescriptor {
        adapter_name: "native",
        default_binary: "builtin-native",
        opencode_compatible: false,
        requires_binary: false,
        capabilities: &[
            "native_loop",
            "typed_tool_engine",
            "schema_validated_tools",
            "scope_policy_enforcement",
            "deterministic_harness",
            "provider_agnostic_contracts",
            "interrupt_runtime",
        ],
        aliases: &["builtin-native"],
    },
];

fn normalize_adapter_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Returns descriptors for built-in runtime adapters.
#[must_use]
pub fn runtime_descriptors() -> &'static [RuntimeDescriptor] {
    &RUNTIME_DESCRIPTORS
}

/// Finds the runtime descriptor for a given adapter/alias, if any.
#[must_use]
pub fn runtime_descriptor_for(name: &str) -> Option<&'static RuntimeDescriptor> {
    if name.trim().is_empty() {
        return None;
    }
    RUNTIME_DESCRIPTORS.iter().find(|descriptor| descriptor.matches(name))
}

/// Resolves the canonical adapter name for a given input.
#[must_use]
pub fn canonical_runtime_adapter_name(name: &str) -> Option<&'static str> {
    runtime_descriptor_for(name).map(|descriptor| descriptor.adapter_name)
}

/// Returns the default binary path for a given adapter/alias, if known.
#[must_use]
pub fn default_runtime_binary(name: &str) -> Option<&'static str> {
    runtime_descriptor_for(name).map(|descriptor| descriptor.default_binary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_adapters_match_descriptor_names() {
        let descriptor_names: Vec<&str> = runtime_descriptors()
            .iter()
            .map(|descriptor| descriptor.adapter_name)
            .collect();
        assert_eq!(descriptor_names, SUPPORTED_ADAPTERS);
    }

    #[test]
    fn aliases_resolve_to_canonical_descriptor() {
        let descriptor = runtime_descriptor_for("ClAuDe").expect("should resolve alias");
        assert_eq!(descriptor.adapter_name, "claude-code");

        let descriptor = runtime_descriptor_for("builtin-native").expect("native alias");
        assert_eq!(descriptor.adapter_name, "native");
    }
}
