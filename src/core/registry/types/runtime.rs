use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeExecuteMode {
    #[default]
    Local,
    Pr,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MergeExecuteOptions {
    #[serde(default)]
    pub mode: MergeExecuteMode,
    #[serde(default)]
    pub monitor_ci: bool,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default)]
    pub pull_after: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeListEntry {
    pub adapter_name: String,
    pub default_binary: String,
    pub available: bool,
    pub opencode_compatible: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealthStatus {
    pub adapter_name: String,
    pub binary_path: String,
    pub healthy: bool,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub selection_source: Option<crate::core::events::RuntimeSelectionSource>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}
