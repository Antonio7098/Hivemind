use super::*;

/// Native managed network proxy mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeNetworkProxyMode {
    Off,
    Managed,
}

impl NativeNetworkProxyMode {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Managed => "managed",
        }
    }
}

/// Native network access envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeNetworkAccessMode {
    Full,
    Limited,
    Disabled,
}

impl NativeNetworkAccessMode {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Limited => "limited",
            Self::Disabled => "disabled",
        }
    }
}

/// Native network approval modes for host/protocol egress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeNetworkApprovalMode {
    None,
    Immediate,
    Deferred,
}

impl NativeNetworkApprovalMode {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Immediate => "immediate",
            Self::Deferred => "deferred",
        }
    }
}

/// Deterministic review decision source for network approvals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeNetworkApprovalDecision {
    Approve,
    Deny,
}

impl NativeNetworkApprovalDecision {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Deny => "deny",
        }
    }
}

/// Native network policy contract for one runtime invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNetworkPolicy {
    pub proxy_mode: NativeNetworkProxyMode,
    pub proxy_http_bind: String,
    pub proxy_socks5_bind: String,
    pub proxy_socks5_enabled: bool,
    pub proxy_admin_bind: String,
    pub proxy_allow_non_loopback: bool,
    pub allowlist: Vec<String>,
    pub denylist: Vec<String>,
    pub block_private_addresses: bool,
    pub access_mode: NativeNetworkAccessMode,
    pub limited_methods: Vec<String>,
    pub approval_mode: NativeNetworkApprovalMode,
    pub approval_decision: NativeNetworkApprovalDecision,
    pub approval_cache_max_entries: usize,
    pub deferred_decisions_file: Option<String>,
}

impl Default for NativeNetworkPolicy {
    fn default() -> Self {
        Self {
            proxy_mode: NativeNetworkProxyMode::Off,
            proxy_http_bind: "127.0.0.1:0".to_string(),
            proxy_socks5_bind: "127.0.0.1:0".to_string(),
            proxy_socks5_enabled: false,
            proxy_admin_bind: "127.0.0.1:0".to_string(),
            proxy_allow_non_loopback: false,
            allowlist: Vec::new(),
            denylist: Vec::new(),
            block_private_addresses: true,
            access_mode: NativeNetworkAccessMode::Full,
            limited_methods: vec!["GET".to_string(), "HEAD".to_string(), "CONNECT".to_string()],
            approval_mode: NativeNetworkApprovalMode::None,
            approval_decision: NativeNetworkApprovalDecision::Deny,
            approval_cache_max_entries: 64,
            deferred_decisions_file: None,
        }
    }
}

impl NativeNetworkPolicy {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut policy = Self::default();
        if let Some(raw) = env.get(NETWORK_PROXY_MODE_ENV_KEY) {
            if let Some(mode) = parse_network_proxy_mode(raw) {
                policy.proxy_mode = mode;
            }
        }
        if let Some(raw) = env.get(NETWORK_PROXY_HTTP_BIND_ENV_KEY) {
            policy.proxy_http_bind = raw.trim().to_string();
        }
        if let Some(raw) = env.get(NETWORK_PROXY_SOCKS5_BIND_ENV_KEY) {
            policy.proxy_socks5_bind = raw.trim().to_string();
        }
        if let Some(raw) = env.get(NETWORK_PROXY_SOCKS5_ENABLED_ENV_KEY) {
            policy.proxy_socks5_enabled = parse_bool_with_default(raw, false);
        }
        if let Some(raw) = env.get(NETWORK_PROXY_ADMIN_BIND_ENV_KEY) {
            policy.proxy_admin_bind = raw.trim().to_string();
        }
        if let Some(raw) = env.get(NETWORK_PROXY_ALLOW_NON_LOOPBACK_ENV_KEY) {
            policy.proxy_allow_non_loopback = parse_bool_with_default(raw, false);
        }
        if let Some(raw) = env.get(NETWORK_ALLOWLIST_ENV_KEY) {
            policy.allowlist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get(NETWORK_DENYLIST_ENV_KEY) {
            policy.denylist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get(NETWORK_BLOCK_PRIVATE_ENV_KEY) {
            policy.block_private_addresses = parse_bool_with_default(raw, true);
        }
        if let Some(raw) = env.get(NETWORK_MODE_ENV_KEY) {
            if let Some(mode) = parse_network_access_mode(raw) {
                policy.access_mode = mode;
            }
        }
        if let Some(raw) = env.get(NETWORK_LIMITED_METHODS_ENV_KEY) {
            let parsed = parse_csv_list(raw)
                .into_iter()
                .map(|method| method.to_ascii_uppercase())
                .collect::<Vec<_>>();
            if !parsed.is_empty() {
                policy.limited_methods = parsed;
            }
        }
        if let Some(raw) = env.get(NETWORK_APPROVAL_MODE_ENV_KEY) {
            if let Some(mode) = parse_network_approval_mode(raw) {
                policy.approval_mode = mode;
            }
        }
        if let Some(raw) = env.get(NETWORK_APPROVAL_DECISION_ENV_KEY) {
            policy.approval_decision = if raw.trim().eq_ignore_ascii_case("approve") {
                NativeNetworkApprovalDecision::Approve
            } else {
                NativeNetworkApprovalDecision::Deny
            };
        }
        if let Some(raw) = env.get(NETWORK_APPROVAL_CACHE_MAX_ENV_KEY) {
            policy.approval_cache_max_entries = raw
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|value| *value > 0)
                .unwrap_or(64);
        }
        if let Some(raw) = env.get(NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE_ENV_KEY) {
            let path = raw.trim();
            if !path.is_empty() {
                policy.deferred_decisions_file = Some(path.to_string());
            }
        }
        policy
    }

    pub(super) fn base_policy_tags(&self) -> Vec<String> {
        vec![
            format!("network_proxy_mode:{}", self.proxy_mode.as_policy_value()),
            format!("network_access_mode:{}", self.access_mode.as_policy_value()),
            format!("network_block_private:{}", self.block_private_addresses),
            format!(
                "network_approval_mode:{}",
                self.approval_mode.as_policy_value()
            ),
            format!(
                "network_approval_decision:{}",
                self.approval_decision.as_policy_value()
            ),
            format!("network_allowlist:{}", self.allowlist.join("|")),
            format!("network_denylist:{}", self.denylist.join("|")),
            format!("network_limited_methods:{}", self.limited_methods.join("|")),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeNetworkTarget {
    pub(super) protocol: String,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) method: String,
}

impl NativeNetworkTarget {
    pub(super) fn cache_key(&self) -> String {
        format!(
            "{}://{}:{}",
            self.protocol.to_ascii_lowercase(),
            self.host.to_ascii_lowercase(),
            self.port
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeDeferredNetworkDecision {
    pub(super) target_key: String,
    pub(super) deny: bool,
}

#[derive(Debug, Default)]
pub struct NativeNetworkApprovalCache {
    approved_for_session: VecDeque<String>,
}

impl NativeNetworkApprovalCache {
    pub(super) fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    pub(super) fn insert_bounded(&mut self, key: String, max_entries: usize) {
        self.approved_for_session.retain(|item| item != &key);
        self.approved_for_session.push_back(key);
        while self.approved_for_session.len() > max_entries {
            let _ = self.approved_for_session.pop_front();
        }
    }
}

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

    pub(super) fn base_policy_tags(&self) -> Vec<String> {
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

/// Explicit approval modes for tool operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeApprovalMode {
    Never,
    OnFailure,
    OnRequest,
    UnlessTrusted,
}

impl NativeApprovalMode {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::OnFailure => "on-failure",
            Self::OnRequest => "on-request",
            Self::UnlessTrusted => "unless-trusted",
        }
    }
}

/// Deterministic review decision source for non-interactive approval gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeApprovalReviewDecision {
    Approve,
    Deny,
}

impl NativeApprovalReviewDecision {
    #[must_use]
    pub const fn as_policy_value(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Deny => "deny",
        }
    }
}

/// Approval policy contract for one native runtime invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeApprovalPolicy {
    pub mode: NativeApprovalMode,
    pub review_decision: NativeApprovalReviewDecision,
    pub trusted_prefixes: Vec<String>,
    pub cache_max_entries: usize,
}

impl Default for NativeApprovalPolicy {
    fn default() -> Self {
        Self {
            mode: NativeApprovalMode::Never,
            review_decision: NativeApprovalReviewDecision::Deny,
            trusted_prefixes: vec![
                "echo".to_string(),
                "git status".to_string(),
                "git diff".to_string(),
            ],
            cache_max_entries: 32,
        }
    }
}

impl NativeApprovalPolicy {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut policy = Self::default();
        if let Some(raw) = env.get(APPROVAL_MODE_ENV_KEY) {
            if let Some(mode) = parse_approval_mode(raw) {
                policy.mode = mode;
            }
        }
        if let Some(raw) = env.get(APPROVAL_REVIEW_DECISION_ENV_KEY) {
            policy.review_decision = if raw.trim().eq_ignore_ascii_case("approve") {
                NativeApprovalReviewDecision::Approve
            } else {
                NativeApprovalReviewDecision::Deny
            };
        }
        if let Some(raw) = env.get(APPROVAL_TRUSTED_PREFIXES_ENV_KEY) {
            policy.trusted_prefixes = parse_csv_list(raw);
        }
        if let Some(raw) = env.get(APPROVAL_CACHE_MAX_ENV_KEY) {
            policy.cache_max_entries = raw
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(32);
        }
        policy
    }
}

#[derive(Debug, Default)]
pub struct NativeApprovalCache {
    approved_for_session: VecDeque<String>,
}

impl NativeApprovalCache {
    pub(super) fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    pub(super) fn matches_command_line(&self, command_line: &str) -> bool {
        self.approved_for_session
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
    }

    pub(super) fn insert_bounded(&mut self, key: String, max_entries: usize) {
        self.approved_for_session.retain(|item| item != &key);
        self.approved_for_session.push_back(key);
        while self.approved_for_session.len() > max_entries {
            let _ = self.approved_for_session.pop_front();
        }
    }
}

/// Command policy for `run_command`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCommandPolicy {
    pub allowlist: Vec<String>,
    pub denylist: Vec<String>,
    pub deny_by_default: bool,
}

impl Default for NativeCommandPolicy {
    fn default() -> Self {
        Self {
            allowlist: Vec::new(),
            denylist: vec!["rm".to_string(), "sudo".to_string()],
            deny_by_default: true,
        }
    }
}

impl NativeCommandPolicy {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut policy = Self::default();
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST") {
            policy.allowlist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENYLIST") {
            policy.denylist = parse_csv_list(raw);
        }
        if let Some(raw) = env.get("HIVEMIND_NATIVE_TOOL_RUN_COMMAND_DENY_BY_DEFAULT") {
            policy.deny_by_default = parse_bool_with_default(raw, true);
        }
        policy
    }

    #[must_use]
    pub fn is_allowed(&self, command_line: &str) -> bool {
        if self
            .denylist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return false;
        }
        if self
            .allowlist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return true;
        }
        !self.deny_by_default
    }
}

/// Rules-backed execution policy manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeExecPolicyManager {
    pub base: NativeCommandPolicy,
    pub prefix_rule_max: usize,
    pub prefix_amendments: Vec<String>,
}

impl Default for NativeExecPolicyManager {
    fn default() -> Self {
        Self {
            base: NativeCommandPolicy::default(),
            prefix_rule_max: 16,
            prefix_amendments: Vec::new(),
        }
    }
}

impl NativeExecPolicyManager {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let mut manager = Self {
            base: NativeCommandPolicy::from_env(env),
            ..Self::default()
        };
        if let Some(raw) = env.get(EXEC_PREFIX_RULE_MAX_ENV_KEY) {
            manager.prefix_rule_max = raw
                .trim()
                .parse::<usize>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(16);
        }
        if let Some(raw) = env.get(EXEC_PREFIX_AMENDMENTS_ENV_KEY) {
            manager.prefix_amendments = parse_csv_list(raw)
                .into_iter()
                .filter(|prefix| !is_broad_prefix(prefix))
                .take(manager.prefix_rule_max)
                .collect();
        }
        manager
    }

    pub(super) fn is_allowed(
        &self,
        command_line: &str,
        approval_cache: &NativeApprovalCache,
    ) -> bool {
        if self
            .base
            .denylist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return false;
        }
        if self
            .base
            .allowlist
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return true;
        }
        if self
            .prefix_amendments
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
            return true;
        }
        if approval_cache.matches_command_line(command_line) {
            return true;
        }
        !self.base.deny_by_default
    }
}

/// Execution context passed to tool handlers.
pub struct ToolExecutionContext<'a> {
    pub worktree: &'a Path,
    pub scope: Option<&'a Scope>,
    pub sandbox_policy: NativeSandboxPolicy,
    pub approval_policy: NativeApprovalPolicy,
    pub network_policy: NativeNetworkPolicy,
    pub command_policy: NativeCommandPolicy,
    pub exec_policy_manager: NativeExecPolicyManager,
    pub approval_cache: RefCell<NativeApprovalCache>,
    pub network_approval_cache: RefCell<NativeNetworkApprovalCache>,
    pub env: &'a HashMap<String, String>,
}

pub(super) fn matches_command_pattern(pattern: &str, command_line: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    pattern == "*"
        || command_line == pattern
        || command_line.starts_with(&format!("{pattern} "))
        || command_line.starts_with(&format!("{pattern}/"))
}

pub(super) fn normalize_relative_path(
    raw: &str,
    allow_empty: bool,
) -> Result<PathBuf, NativeToolEngineError> {
    let mut normalized = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(NativeToolEngineError::validation(format!(
                    "invalid relative path '{raw}'"
                )));
            }
        }
    }

    if normalized.as_os_str().is_empty() && !allow_empty {
        return Err(NativeToolEngineError::validation(
            "path cannot be empty or current-directory only",
        ));
    }

    Ok(normalized)
}

pub(super) fn relative_display(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".to_string()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}
