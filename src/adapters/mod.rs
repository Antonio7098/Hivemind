//! Runtime adapters for agent execution.

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
