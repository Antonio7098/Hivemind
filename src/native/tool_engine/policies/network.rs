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

    pub(crate) fn base_policy_tags(&self) -> Vec<String> {
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
pub(crate) struct NativeNetworkTarget {
    pub(crate) protocol: String,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) method: String,
}

impl NativeNetworkTarget {
    pub(crate) fn cache_key(&self) -> String {
        format!(
            "{}://{}:{}",
            self.protocol.to_ascii_lowercase(),
            self.host.to_ascii_lowercase(),
            self.port
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeDeferredNetworkDecision {
    pub(crate) target_key: String,
    pub(crate) deny: bool,
}

#[derive(Debug, Default)]
pub struct NativeNetworkApprovalCache {
    approved_for_session: VecDeque<String>,
}

impl NativeNetworkApprovalCache {
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    pub(crate) fn insert_bounded(&mut self, key: String, max_entries: usize) {
        self.approved_for_session.retain(|item| item != &key);
        self.approved_for_session.push_back(key);
        while self.approved_for_session.len() > max_entries {
            let _ = self.approved_for_session.pop_front();
        }
    }
}
