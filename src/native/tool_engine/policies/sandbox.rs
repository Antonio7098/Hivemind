use super::*;

/// Explicit sandbox modes for native tool execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeSandboxMode {
    ReadOnly,
    WorkspaceWrite,
    DangerFullAccess,
    HostPassthrough,
}

impl NativeSandboxMode {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::DangerFullAccess => "danger-full-access",
            Self::HostPassthrough => "host-passthrough",
        }
    }
}

/// Sandbox policy contract resolved for one native runtime invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSandboxPolicy {
    pub mode: NativeSandboxMode,
    pub writable_roots: Vec<String>,
    pub read_only_overlays: Vec<String>,
    pub platform: String,
    pub selection: String,
}

impl Default for NativeSandboxPolicy {
    fn default() -> Self {
        let mode = default_sandbox_mode_for_platform();
        let writable_roots = if mode == NativeSandboxMode::WorkspaceWrite {
            vec![".".to_string()]
        } else {
            Vec::new()
        };
        Self {
            mode,
            writable_roots,
            read_only_overlays: Vec::new(),
            platform: current_platform_tag().to_string(),
            selection: "platform_default".to_string(),
        }
    }
}

impl NativeSandboxPolicy {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut policy = Self::default();
        if let Some(raw) = env.get(SANDBOX_MODE_ENV_KEY) {
            if let Some(mode) = parse_sandbox_mode(raw) {
                policy.mode = mode;
                policy.selection = "env_override".to_string();
                policy.writable_roots = if mode == NativeSandboxMode::WorkspaceWrite {
                    vec![".".to_string()]
                } else {
                    Vec::new()
                };
            }
        }
        if let Some(raw) = env.get(SANDBOX_WRITABLE_ROOTS_ENV_KEY) {
            policy.writable_roots = parse_csv_list(raw)
                .into_iter()
                .filter_map(|item| normalize_relative_path(&item, true).ok())
                .map(|path| relative_display(&path))
                .collect();
        }
        if let Some(raw) = env.get(SANDBOX_READ_ONLY_OVERLAYS_ENV_KEY) {
            policy.read_only_overlays = parse_csv_list(raw)
                .into_iter()
                .filter_map(|item| normalize_relative_path(&item, true).ok())
                .map(|path| relative_display(&path))
                .collect();
        }
        policy
    }

    pub(crate) fn base_policy_tags(&self) -> Vec<String> {
        vec![
            format!("sandbox_mode:{}", self.mode.as_policy_value()),
            format!("sandbox_platform:{}", self.platform),
            format!("sandbox_selection:{}", self.selection),
            format!("sandbox_writable_roots:{}", self.writable_roots.join("|")),
            format!(
                "sandbox_read_only_overlays:{}",
                self.read_only_overlays.join("|")
            ),
        ]
    }
}
