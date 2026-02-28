//! Native typed tool engine with schema validation and policy-aware dispatch.
//!
//! Sprint 44 introduces a deterministic tool registry for native runtime turns.

use crate::adapters::runtime::{
    deterministic_env_pairs, NativeToolCallFailure, NativeToolCallTrace,
};
use crate::core::graph_query::{
    load_partition_paths_from_constitution, GraphQueryBounds, GraphQueryIndex,
    GraphQueryRepository, GraphQueryRequest, GraphQueryResult, GRAPH_QUERY_ENV_CONSTITUTION_PATH,
    GRAPH_QUERY_ENV_GATE_ERROR, GRAPH_QUERY_ENV_SNAPSHOT_PATH, GRAPH_QUERY_REFRESH_HINT,
};
use crate::core::scope::Scope;
use jsonschema::JSONSchema;
use schemars::{schema_for, JsonSchema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use ucp_api::{canonical_fingerprint, PortableDocument, CODEGRAPH_PROFILE_MARKER};

const TOOL_VERSION_V1: &str = "1.0.0";
const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// Structured native tool engine error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolEngineError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub policy_tags: Vec<String>,
}

impl NativeToolEngineError {
    fn new(code: impl Into<String>, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
            policy_tags: Vec::new(),
        }
    }

    fn with_policy_tags(mut self, policy_tags: Vec<String>) -> Self {
        self.policy_tags = policy_tags;
        self
    }

    fn unknown_tool(tool_name: &str) -> Self {
        Self::new(
            "native_tool_unknown",
            format!("Unknown native tool '{tool_name}'"),
            false,
        )
    }

    fn validation(error: impl Into<String>) -> Self {
        Self::new("native_tool_input_invalid", error, false)
    }

    fn output_validation(tool_name: &str, error: impl Into<String>) -> Self {
        Self::new(
            "native_tool_output_invalid",
            format!(
                "Tool '{tool_name}' produced invalid output: {}",
                error.into()
            ),
            false,
        )
    }

    fn scope_violation(error: impl Into<String>) -> Self {
        Self::new("native_scope_violation", error, false)
    }

    fn policy_violation(error: impl Into<String>) -> Self {
        Self::new("native_policy_violation", error, false)
    }

    fn execution(error: impl Into<String>) -> Self {
        Self::new("native_tool_execution_failed", error, false)
    }

    fn timeout(tool_name: &str, timeout_ms: u64) -> Self {
        Self::new(
            "native_tool_timeout",
            format!("Tool '{tool_name}' exceeded timeout envelope ({timeout_ms}ms)"),
            false,
        )
    }
}

/// High-level capability requirement for a tool contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPermission {
    FilesystemRead,
    FilesystemWrite,
    Execution,
    GitRead,
}

/// Declared tool contract used by the deterministic registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolContract {
    pub name: String,
    pub version: String,
    pub required_scope: String,
    pub required_permissions: Vec<ToolPermission>,
    pub timeout_ms: u64,
    pub cancellable: bool,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// Parsed native tool action from `ACT:` directives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolAction {
    pub name: String,
    #[serde(default = "default_tool_version")]
    pub version: String,
    #[serde(default)]
    pub input: Value,
}

fn default_tool_version() -> String {
    TOOL_VERSION_V1.to_string()
}

impl NativeToolAction {
    /// Parses `tool:` directives in one of these forms:
    /// - `tool:<name>:<json-object>`
    /// - `tool:{"name":"...","version":"...","input":{...}}`
    pub fn parse(action: &str) -> Result<Option<Self>, NativeToolEngineError> {
        let Some(raw) = action.trim().strip_prefix("tool:") else {
            return Ok(None);
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool action is missing tool name/payload",
            ));
        }

        if raw.starts_with('{') {
            return serde_json::from_str::<Self>(raw)
                .map(Some)
                .map_err(|error| {
                    NativeToolEngineError::validation(format!(
                        "tool action JSON payload is invalid: {error}"
                    ))
                });
        }

        let mut parts = raw.splitn(2, ':');
        let tool_name = parts.next().unwrap_or_default().trim();
        if tool_name.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool name cannot be empty",
            ));
        }

        let input = match parts.next().map(str::trim) {
            None | Some("") => json!({}),
            Some(payload) => serde_json::from_str::<Value>(payload).map_err(|error| {
                NativeToolEngineError::validation(format!(
                    "tool input JSON payload is invalid: {error}"
                ))
            })?,
        };

        Ok(Some(Self {
            name: tool_name.to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input,
        }))
    }
}

const SANDBOX_MODE_ENV_KEY: &str = "HIVEMIND_NATIVE_SANDBOX_MODE";
const SANDBOX_WRITABLE_ROOTS_ENV_KEY: &str = "HIVEMIND_NATIVE_SANDBOX_WRITABLE_ROOTS";
const SANDBOX_READ_ONLY_OVERLAYS_ENV_KEY: &str = "HIVEMIND_NATIVE_SANDBOX_READ_ONLY_OVERLAYS";
const APPROVAL_MODE_ENV_KEY: &str = "HIVEMIND_NATIVE_APPROVAL_MODE";
const APPROVAL_REVIEW_DECISION_ENV_KEY: &str = "HIVEMIND_NATIVE_APPROVAL_REVIEW_DECISION";
const APPROVAL_TRUSTED_PREFIXES_ENV_KEY: &str = "HIVEMIND_NATIVE_APPROVAL_TRUSTED_PREFIXES";
const APPROVAL_CACHE_MAX_ENV_KEY: &str = "HIVEMIND_NATIVE_APPROVAL_CACHE_MAX";
const EXEC_PREFIX_RULE_MAX_ENV_KEY: &str = "HIVEMIND_NATIVE_EXEC_PREFIX_RULE_MAX";
const EXEC_PREFIX_AMENDMENTS_ENV_KEY: &str = "HIVEMIND_NATIVE_EXEC_PREFIX_AMENDMENTS";
const NETWORK_PROXY_MODE_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_PROXY_MODE";
const NETWORK_PROXY_HTTP_BIND_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_PROXY_HTTP_BIND";
const NETWORK_PROXY_SOCKS5_BIND_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_PROXY_SOCKS5_BIND";
const NETWORK_PROXY_SOCKS5_ENABLED_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_PROXY_SOCKS5_ENABLED";
const NETWORK_PROXY_ADMIN_BIND_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_PROXY_ADMIN_BIND";
const NETWORK_PROXY_ALLOW_NON_LOOPBACK_ENV_KEY: &str =
    "HIVEMIND_NATIVE_NETWORK_PROXY_ALLOW_NON_LOOPBACK";
const NETWORK_ALLOWLIST_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_ALLOWLIST";
const NETWORK_DENYLIST_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_DENYLIST";
const NETWORK_BLOCK_PRIVATE_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_BLOCK_PRIVATE";
const NETWORK_MODE_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_MODE";
const NETWORK_LIMITED_METHODS_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_LIMITED_METHODS";
const NETWORK_APPROVAL_MODE_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_APPROVAL_MODE";
const NETWORK_APPROVAL_DECISION_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_APPROVAL_DECISION";
const NETWORK_APPROVAL_CACHE_MAX_ENV_KEY: &str = "HIVEMIND_NATIVE_NETWORK_APPROVAL_CACHE_MAX";
const NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE_ENV_KEY: &str =
    "HIVEMIND_NATIVE_NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE";

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

    fn base_policy_tags(&self) -> Vec<String> {
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
struct NativeNetworkTarget {
    protocol: String,
    host: String,
    port: u16,
    method: String,
}

impl NativeNetworkTarget {
    fn cache_key(&self) -> String {
        format!(
            "{}://{}:{}",
            self.protocol.to_ascii_lowercase(),
            self.host.to_ascii_lowercase(),
            self.port
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeDeferredNetworkDecision {
    target_key: String,
    deny: bool,
}

#[derive(Debug, Default)]
pub struct NativeNetworkApprovalCache {
    approved_for_session: VecDeque<String>,
}

impl NativeNetworkApprovalCache {
    fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    fn insert_bounded(&mut self, key: String, max_entries: usize) {
        self.approved_for_session.retain(|item| item != &key);
        self.approved_for_session.push_back(key);
        while self.approved_for_session.len() > max_entries {
            let _ = self.approved_for_session.pop_front();
        }
    }
}

struct ManagedProxyRuntime {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    http_addr: String,
    socks5_addr: Option<String>,
    admin_addr: String,
}

impl ManagedProxyRuntime {
    fn start(policy: &NativeNetworkPolicy) -> Result<Self, NativeToolEngineError> {
        let (http_bind, _http_clamped) = clamp_bind_address(
            &policy.proxy_http_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (admin_bind, _admin_clamped) = clamp_bind_address(
            &policy.proxy_admin_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (socks_bind, _socks_clamped) = clamp_bind_address(
            &policy.proxy_socks5_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );

        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();

        let http_listener = TcpListener::bind(&http_bind).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to bind managed HTTP proxy listener on '{http_bind}': {error}"
            ))
        })?;
        http_listener.set_nonblocking(true).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to configure managed HTTP proxy listener nonblocking mode: {error}"
            ))
        })?;
        let http_addr = http_listener.local_addr().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read managed HTTP proxy listener address: {error}"
            ))
        })?;
        handles.push(spawn_proxy_listener(
            stop.clone(),
            http_listener,
            false,
            format!("http://{http_addr}"),
        ));

        let mut socks5_addr = None;
        if policy.proxy_socks5_enabled {
            let socks_listener = TcpListener::bind(&socks_bind).map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to bind managed SOCKS5 listener on '{socks_bind}': {error}"
                ))
            })?;
            socks_listener.set_nonblocking(true).map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to configure managed SOCKS5 listener nonblocking mode: {error}"
                ))
            })?;
            let addr = socks_listener.local_addr().map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to read managed SOCKS5 listener address: {error}"
                ))
            })?;
            let rendered = format!("socks5://{addr}");
            socks5_addr = Some(rendered.clone());
            handles.push(spawn_proxy_listener(
                stop.clone(),
                socks_listener,
                true,
                rendered,
            ));
        }

        let admin_listener = TcpListener::bind(&admin_bind).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to bind managed proxy admin listener on '{admin_bind}': {error}"
            ))
        })?;
        admin_listener.set_nonblocking(true).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to configure managed admin listener nonblocking mode: {error}"
            ))
        })?;
        let admin_addr = admin_listener.local_addr().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read managed proxy admin address: {error}"
            ))
        })?;
        handles.push(spawn_admin_listener(
            stop.clone(),
            admin_listener,
            format!("http://{admin_addr}"),
            format!("http://{http_addr}"),
            socks5_addr.clone(),
        ));

        Ok(Self {
            stop,
            handles,
            http_addr: format!("http://{http_addr}"),
            socks5_addr,
            admin_addr: format!("http://{admin_addr}"),
        })
    }

    fn apply_proxy_env(&self, cmd: &mut Command) {
        cmd.env("HTTP_PROXY", &self.http_addr);
        cmd.env("HTTPS_PROXY", &self.http_addr);
        cmd.env("NO_PROXY", "localhost,127.0.0.1,::1");
        cmd.env("HIVEMIND_NATIVE_NETWORK_PROXY_ADMIN", &self.admin_addr);
        if let Some(socks5) = self.socks5_addr.as_ref() {
            cmd.env("ALL_PROXY", socks5);
        }
    }
}

impl Drop for ManagedProxyRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
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

    fn base_policy_tags(&self) -> Vec<String> {
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
    fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    fn insert_bounded(&mut self, key: String, max_entries: usize) {
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

    fn is_allowed(&self, command_line: &str, approval_cache: &NativeApprovalCache) -> bool {
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
        if approval_cache
            .approved_for_session
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
        {
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

type ToolHandler =
    fn(&ToolExecutionContext<'_>, &Value, u64) -> Result<Value, NativeToolEngineError>;

struct RegisteredTool {
    contract: ToolContract,
    input_validator: JSONSchema,
    output_validator: JSONSchema,
    handler: ToolHandler,
}

/// Deterministic, typed native tool registry and dispatcher.
pub struct NativeToolEngine {
    tools: BTreeMap<String, RegisteredTool>,
}

impl Default for NativeToolEngine {
    fn default() -> Self {
        Self::new().unwrap_or_else(|error| {
            panic!(
                "native tool engine bootstrap failed ({}): {}",
                error.code, error.message
            )
        })
    }
}

impl NativeToolEngine {
    pub fn new() -> Result<Self, NativeToolEngineError> {
        let mut engine = Self {
            tools: BTreeMap::new(),
        };
        engine.register_builtin::<ReadFileInput, ReadFileOutput>(
            "read_file",
            "filesystem_read",
            vec![ToolPermission::FilesystemRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_read_file,
        )?;
        engine.register_builtin::<ListFilesInput, ListFilesOutput>(
            "list_files",
            "filesystem_read",
            vec![ToolPermission::FilesystemRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_list_files,
        )?;
        engine.register_builtin::<WriteFileInput, WriteFileOutput>(
            "write_file",
            "filesystem_write",
            vec![ToolPermission::FilesystemWrite],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_write_file,
        )?;
        engine.register_builtin::<RunCommandInput, RunCommandOutput>(
            "run_command",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            false,
            handle_run_command,
        )?;
        engine.register_builtin::<NoInput, GitStatusOutput>(
            "git_status",
            "repository_read",
            vec![ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_git_status,
        )?;
        engine.register_builtin::<GitDiffInput, GitDiffOutput>(
            "git_diff",
            "repository_read",
            vec![ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_git_diff,
        )?;
        engine.register_builtin::<GraphQueryInput, GraphQueryResult>(
            "graph_query",
            "graph_query_read",
            vec![ToolPermission::FilesystemRead, ToolPermission::GitRead],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_graph_query,
        )?;
        Ok(engine)
    }

    fn register_builtin<I, O>(
        &mut self,
        name: &str,
        required_scope: &str,
        required_permissions: Vec<ToolPermission>,
        timeout_ms: u64,
        cancellable: bool,
        handler: ToolHandler,
    ) -> Result<(), NativeToolEngineError>
    where
        I: JsonSchema,
        O: JsonSchema,
    {
        let input_schema = serde_json::to_value(schema_for!(I)).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to encode input schema for '{name}': {error}"
            ))
        })?;
        let output_schema = serde_json::to_value(schema_for!(O)).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to encode output schema for '{name}': {error}"
            ))
        })?;
        let input_validator = JSONSchema::compile(&input_schema).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to compile input schema for '{name}': {error}"
            ))
        })?;
        let output_validator = JSONSchema::compile(&output_schema).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to compile output schema for '{name}': {error}"
            ))
        })?;

        let contract = ToolContract {
            name: name.to_string(),
            version: TOOL_VERSION_V1.to_string(),
            required_scope: required_scope.to_string(),
            required_permissions,
            timeout_ms,
            cancellable,
            input_schema,
            output_schema,
        };

        self.tools.insert(
            Self::tool_key(name, TOOL_VERSION_V1),
            RegisteredTool {
                contract,
                input_validator,
                output_validator,
                handler,
            },
        );
        Ok(())
    }

    fn tool_key(name: &str, version: &str) -> String {
        format!("{name}@{version}")
    }

    fn validate_input(
        validator: &JSONSchema,
        tool_name: &str,
        payload: &Value,
    ) -> Result<(), NativeToolEngineError> {
        validator.validate(payload).map_err(|iter| {
            let first = iter.into_iter().next();
            let message = first.map_or_else(
                || "schema validation failed".to_string(),
                |error| format!("path '{}' violated schema: {}", error.instance_path, error),
            );
            NativeToolEngineError::validation(format!(
                "tool '{tool_name}' input is invalid: {message}"
            ))
        })
    }

    fn validate_output(
        validator: &JSONSchema,
        tool_name: &str,
        payload: &Value,
    ) -> Result<(), NativeToolEngineError> {
        validator.validate(payload).map_err(|iter| {
            let first = iter.into_iter().next();
            let message = first.map_or_else(
                || "schema validation failed".to_string(),
                |error| format!("path '{}' violated schema: {}", error.instance_path, error),
            );
            NativeToolEngineError::output_validation(tool_name, message)
        })
    }

    #[allow(clippy::too_many_lines)]
    fn evaluate_tool_policies(
        action: &NativeToolAction,
        tool: &RegisteredTool,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<Vec<String>, NativeToolEngineError> {
        let mut tags = ctx.sandbox_policy.base_policy_tags();
        tags.push(format!(
            "approval_mode:{}",
            ctx.approval_policy.mode.as_policy_value()
        ));

        let requires_write = tool
            .contract
            .required_permissions
            .iter()
            .any(|perm| matches!(perm, ToolPermission::FilesystemWrite));
        let requires_exec = tool
            .contract
            .required_permissions
            .iter()
            .any(|perm| matches!(perm, ToolPermission::Execution));

        match ctx.sandbox_policy.mode {
            NativeSandboxMode::ReadOnly if requires_write || requires_exec => {
                tags.push("sandbox_decision:denied".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "tool '{}' denied by sandbox policy '{}' (write/exec blocked)",
                    action.name,
                    ctx.sandbox_policy.mode.as_policy_value()
                ))
                .with_policy_tags(tags));
            }
            NativeSandboxMode::WorkspaceWrite if action.name == "write_file" => {
                let write = decode_input::<WriteFileInput>(&action.input)?;
                let rel = normalize_relative_path(&write.path, false)?;
                let rel_display = relative_display(&rel);
                let roots = if ctx.sandbox_policy.writable_roots.is_empty() {
                    vec![".".to_string()]
                } else {
                    ctx.sandbox_policy.writable_roots.clone()
                };
                let allowed = roots.iter().any(|root| {
                    root == "."
                        || rel_display == *root
                        || rel_display.starts_with(&format!("{root}/"))
                });
                if !allowed {
                    tags.push("sandbox_decision:denied".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "tool 'write_file' denied by sandbox policy '{}': path '{}' is outside writable roots [{}]",
                        ctx.sandbox_policy.mode.as_policy_value(),
                        rel_display,
                        roots.join(", ")
                    ))
                    .with_policy_tags(tags));
                }
                tags.push("sandbox_decision:workspace_write_allow".to_string());
            }
            _ => tags.push("sandbox_decision:allowed".to_string()),
        }

        let mut approval_required = false;
        let mut approval_cache_key = format!("tool:{}", action.name);
        let mut command_line = None;
        let mut dangerous_reason = None;
        if action.name == "run_command" {
            let run = decode_input::<RunCommandInput>(&action.input)?;
            let command = run.command.trim();
            let joined = if run.args.is_empty() {
                command.to_string()
            } else {
                format!("{command} {}", run.args.join(" "))
            };
            dangerous_reason = dangerous_command_reason(command, &run.args);
            approval_cache_key = command.to_ascii_lowercase();
            command_line = Some(joined);
            let network_targets = extract_network_targets(command, &run.args);
            tags.extend(ctx.network_policy.base_policy_tags());
            let (_, http_clamped) = clamp_bind_address(
                &ctx.network_policy.proxy_http_bind,
                ctx.network_policy.proxy_allow_non_loopback,
                "127.0.0.1:0",
            );
            let (_, admin_clamped) = clamp_bind_address(
                &ctx.network_policy.proxy_admin_bind,
                ctx.network_policy.proxy_allow_non_loopback,
                "127.0.0.1:0",
            );
            let (_, socks_clamped) = clamp_bind_address(
                &ctx.network_policy.proxy_socks5_bind,
                ctx.network_policy.proxy_allow_non_loopback,
                "127.0.0.1:0",
            );
            tags.push(format!(
                "network_proxy_bind_clamped:{}",
                http_clamped || admin_clamped || socks_clamped
            ));
            if network_targets.is_empty() {
                tags.push("network_targets:none".to_string());
            } else {
                tags.push(format!("network_targets_count:{}", network_targets.len()));
                for target in &network_targets {
                    tags.push(format!(
                        "network_target:{}",
                        format_network_target_tag_value(target)
                    ));

                    if matches!(
                        ctx.network_policy.access_mode,
                        NativeNetworkAccessMode::Disabled
                    ) {
                        tags.push("network_decision:denied_mode_disabled".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network policy denied '{}' because network mode is disabled",
                            target.cache_key()
                        ))
                        .with_policy_tags(tags));
                    }
                    if ctx
                        .network_policy
                        .denylist
                        .iter()
                        .any(|pattern| matches_host_pattern(pattern, &target.host))
                    {
                        tags.push("network_decision:denied_denylist".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network policy denied '{}' by denylist",
                            target.cache_key()
                        ))
                        .with_policy_tags(tags));
                    }
                    if !ctx.network_policy.allowlist.is_empty()
                        && !ctx
                            .network_policy
                            .allowlist
                            .iter()
                            .any(|pattern| matches_host_pattern(pattern, &target.host))
                    {
                        tags.push("network_decision:denied_not_allowlisted".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network policy denied '{}': host is not in allowlist",
                            target.cache_key()
                        ))
                        .with_policy_tags(tags));
                    }
                    if ctx.network_policy.block_private_addresses
                        && is_private_or_local_host(&target.host)
                    {
                        tags.push("network_decision:denied_private_address".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network policy denied '{}': private/local address blocked",
                            target.cache_key()
                        ))
                        .with_policy_tags(tags));
                    }
                    if matches!(
                        ctx.network_policy.access_mode,
                        NativeNetworkAccessMode::Limited
                    ) && !ctx
                        .network_policy
                        .limited_methods
                        .iter()
                        .any(|method| method.eq_ignore_ascii_case(&target.method))
                    {
                        tags.push("network_decision:denied_method_restricted".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network policy denied '{}': method '{}' is not allowed in limited mode",
                            target.cache_key(),
                            target.method
                        ))
                        .with_policy_tags(tags));
                    }

                    let approval_key = target.cache_key();
                    match ctx.network_policy.approval_mode {
                        NativeNetworkApprovalMode::None => {
                            tags.push("network_approval_required:false".to_string());
                            tags.push("network_approval_outcome:not_required".to_string());
                        }
                        NativeNetworkApprovalMode::Immediate => {
                            tags.push("network_approval_required:true".to_string());
                            let mut cache = ctx.network_approval_cache.borrow_mut();
                            if cache.contains(&approval_key) {
                                tags.push("network_approval_outcome:approved_cached".to_string());
                            } else if matches!(
                                ctx.network_policy.approval_decision,
                                NativeNetworkApprovalDecision::Approve
                            ) {
                                cache.insert_bounded(
                                    approval_key,
                                    ctx.network_policy.approval_cache_max_entries,
                                );
                                tags.push(
                                    "network_approval_outcome:approved_for_session".to_string(),
                                );
                            } else {
                                tags.push("network_approval_outcome:denied".to_string());
                                return Err(NativeToolEngineError::policy_violation(format!(
                                    "network approval denied '{}'",
                                    target.cache_key()
                                ))
                                .with_policy_tags(tags));
                            }
                        }
                        NativeNetworkApprovalMode::Deferred => {
                            tags.push("network_approval_required:true".to_string());
                            if ctx.network_policy.deferred_decisions_file.is_none() {
                                tags.push("network_approval_outcome:denied_no_watcher".to_string());
                                return Err(NativeToolEngineError::policy_violation(format!(
                                    "network approval deferred mode requires {NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE_ENV_KEY}"
                                ))
                                .with_policy_tags(tags));
                            }
                            tags.push("network_approval_outcome:deferred_pending".to_string());
                        }
                    }
                }
                tags.push("network_decision:preflight_allowed".to_string());
            }
            if matches!(
                ctx.network_policy.proxy_mode,
                NativeNetworkProxyMode::Managed
            ) {
                tags.push("network_proxy:managed".to_string());
            } else {
                tags.push("network_proxy:off".to_string());
            }
        }

        let trusted = command_line.as_deref().is_some_and(|line| {
            ctx.approval_policy
                .trusted_prefixes
                .iter()
                .any(|prefix| matches_command_pattern(prefix, line))
        });

        match ctx.approval_policy.mode {
            NativeApprovalMode::Never => {
                tags.push("approval_required:false".to_string());
                tags.push("approval_outcome:not_required".to_string());
            }
            NativeApprovalMode::OnFailure => {
                tags.push("approval_required:false".to_string());
                tags.push("approval_outcome:deferred_on_failure".to_string());
            }
            NativeApprovalMode::OnRequest => {
                approval_required = requires_write || requires_exec;
            }
            NativeApprovalMode::UnlessTrusted => {
                approval_required = (requires_write || requires_exec) && !trusted;
                tags.push(format!("approval_trusted_prefix:{trusted}"));
            }
        }
        if dangerous_reason.is_some() {
            approval_required = true;
            tags.push("exec_dangerous:true".to_string());
        } else {
            tags.push("exec_dangerous:false".to_string());
        }

        if approval_required {
            tags.push("approval_required:true".to_string());
            if is_broad_prefix(&approval_cache_key) {
                tags.push("approval_outcome:denied_broad_prefix".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "approval denied for broad prefix '{approval_cache_key}'; provide a specific command prefix"
                ))
                .with_policy_tags(tags));
            }

            let mut cache = ctx.approval_cache.borrow_mut();
            if cache.contains(&approval_cache_key) {
                tags.push("approval_review_decision:cached".to_string());
                tags.push("approval_outcome:approved_for_session".to_string());
                tags.push("approved_for_session:true".to_string());
            } else if ctx.approval_policy.review_decision == NativeApprovalReviewDecision::Approve {
                cache.insert_bounded(approval_cache_key, ctx.approval_policy.cache_max_entries);
                tags.push("approval_review_decision:approve".to_string());
                tags.push("approval_outcome:approved_for_session".to_string());
                tags.push("approved_for_session:true".to_string());
            } else {
                tags.push("approval_review_decision:deny".to_string());
                tags.push("approval_outcome:denied".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "approval denied for '{}' under mode '{}'",
                    approval_cache_key,
                    ctx.approval_policy.mode.as_policy_value()
                ))
                .with_policy_tags(tags));
            }
        }

        if let Some(reason) = dangerous_reason {
            tags.push(format!(
                "exec_danger_reason:{}",
                sanitize_policy_tag_value(&reason)
            ));
            if !matches!(
                ctx.sandbox_policy.mode,
                NativeSandboxMode::DangerFullAccess | NativeSandboxMode::HostPassthrough
            ) {
                tags.push("exec_decision:denied_dangerous_requires_elevation".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "dangerous command denied: {reason}. Set {SANDBOX_MODE_ENV_KEY}=danger-full-access (or host-passthrough) and request approval."
                ))
                .with_policy_tags(tags));
            }
            if matches!(
                ctx.approval_policy.mode,
                NativeApprovalMode::Never | NativeApprovalMode::OnFailure
            ) {
                tags.push("exec_decision:denied_dangerous_requires_preapproval".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "dangerous command denied: {reason}. Set {APPROVAL_MODE_ENV_KEY} to on-request or unless-trusted."
                ))
                .with_policy_tags(tags));
            }
        }

        if let Some(line) = command_line.as_deref() {
            let cache = ctx.approval_cache.borrow();
            if !ctx.exec_policy_manager.is_allowed(line, &cache)
                || !ctx.command_policy.is_allowed(line)
            {
                tags.push("exec_decision:denied_exec_policy".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "run_command blocked by execution policy: {line}"
                ))
                .with_policy_tags(tags));
            }
            tags.push("exec_decision:allowed_exec_policy".to_string());
        }

        Ok(tags)
    }

    fn execute_internal(
        &self,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<(Value, Vec<String>), NativeToolEngineError> {
        let tool_key = Self::tool_key(&action.name, &action.version);
        let Some(tool) = self.tools.get(&tool_key) else {
            return Err(NativeToolEngineError::unknown_tool(&action.name));
        };

        Self::validate_input(&tool.input_validator, &action.name, &action.input)?;
        let policy_tags = Self::evaluate_tool_policies(action, tool, ctx)?;
        let started = Instant::now();
        let output = (tool.handler)(ctx, &action.input, tool.contract.timeout_ms)?;
        if started.elapsed() > Duration::from_millis(tool.contract.timeout_ms) {
            return Err(NativeToolEngineError::timeout(
                &action.name,
                tool.contract.timeout_ms,
            ));
        }
        Self::validate_output(&tool.output_validator, &action.name, &output)?;
        Ok((output, policy_tags))
    }

    pub fn execute(
        &self,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> Result<Value, NativeToolEngineError> {
        self.execute_internal(action, ctx).map(|(output, _)| output)
    }

    pub fn execute_action_trace(
        &self,
        call_id: String,
        action: &NativeToolAction,
        ctx: &ToolExecutionContext<'_>,
    ) -> NativeToolCallTrace {
        let request_payload = json!({
            "tool": action.name,
            "version": action.version,
            "input": action.input
        });
        let request = serde_json::to_string(&request_payload).unwrap_or_else(|error| {
            format!(
                "{{\"tool\":\"{}\",\"encode_error\":\"{}\"}}",
                action.name, error
            )
        });

        match self.execute_internal(action, ctx) {
            Ok((output, policy_tags)) => {
                let response_payload = json!({
                    "ok": true,
                    "output": output,
                });
                NativeToolCallTrace {
                    call_id,
                    tool_name: action.name.clone(),
                    request,
                    response: Some(serde_json::to_string(&response_payload).unwrap_or_else(
                        |error| format!("{{\"ok\":false,\"encode_error\":\"{error}\"}}"),
                    )),
                    failure: None,
                    policy_tags,
                }
            }
            Err(error) => NativeToolCallTrace {
                call_id,
                tool_name: action.name.clone(),
                request,
                response: None,
                failure: Some(NativeToolCallFailure {
                    code: error.code.clone(),
                    message: error.message.clone(),
                    recoverable: error.recoverable,
                }),
                policy_tags: error.policy_tags,
            },
        }
    }
}

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn parse_bool_with_default(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

fn current_platform_tag() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

fn default_sandbox_mode_for_platform() -> NativeSandboxMode {
    if current_platform_tag() == "windows" {
        NativeSandboxMode::HostPassthrough
    } else {
        NativeSandboxMode::WorkspaceWrite
    }
}

fn parse_sandbox_mode(raw: &str) -> Option<NativeSandboxMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "read-only" | "read_only" => Some(NativeSandboxMode::ReadOnly),
        "workspace-write" | "workspace_write" => Some(NativeSandboxMode::WorkspaceWrite),
        "danger-full-access" | "danger_full_access" => Some(NativeSandboxMode::DangerFullAccess),
        "host-passthrough" | "host_passthrough" | "host" => {
            Some(NativeSandboxMode::HostPassthrough)
        }
        _ => None,
    }
}

fn parse_approval_mode(raw: &str) -> Option<NativeApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "never" => Some(NativeApprovalMode::Never),
        "on-failure" | "on_failure" => Some(NativeApprovalMode::OnFailure),
        "on-request" | "on_request" => Some(NativeApprovalMode::OnRequest),
        "unless-trusted" | "unless_trusted" => Some(NativeApprovalMode::UnlessTrusted),
        _ => None,
    }
}

fn parse_network_proxy_mode(raw: &str) -> Option<NativeNetworkProxyMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => Some(NativeNetworkProxyMode::Off),
        "managed" => Some(NativeNetworkProxyMode::Managed),
        _ => None,
    }
}

fn parse_network_access_mode(raw: &str) -> Option<NativeNetworkAccessMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "full" => Some(NativeNetworkAccessMode::Full),
        "limited" => Some(NativeNetworkAccessMode::Limited),
        "disabled" | "none" | "deny-all" | "deny_all" => Some(NativeNetworkAccessMode::Disabled),
        _ => None,
    }
}

fn parse_network_approval_mode(raw: &str) -> Option<NativeNetworkApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => Some(NativeNetworkApprovalMode::None),
        "immediate" => Some(NativeNetworkApprovalMode::Immediate),
        "deferred" => Some(NativeNetworkApprovalMode::Deferred),
        _ => None,
    }
}

fn spawn_proxy_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    is_socks5: bool,
    endpoint: String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if is_socks5 {
                        let _ = stream.write_all(&[0x05, 0xFF]);
                    } else {
                        let _ = respond_proxy_denied(&mut stream, &endpoint);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

fn spawn_admin_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    endpoint: String,
    http_proxy: String,
    socks_proxy: Option<String>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let body = serde_json::to_string(&json!({
                        "ok": true,
                        "admin_endpoint": endpoint,
                        "http_proxy": http_proxy,
                        "socks5_proxy": socks_proxy,
                    }))
                    .unwrap_or_else(|_| "{\"ok\":false}".to_string());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

fn respond_proxy_denied(stream: &mut TcpStream, endpoint: &str) -> std::io::Result<()> {
    let body = format!(
        "{{\"ok\":false,\"code\":\"network_proxy_denied\",\"message\":\"managed network proxy denied request\",\"admin\":\"{endpoint}\"}}"
    );
    let response = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

fn clamp_bind_address(raw: &str, allow_non_loopback: bool, fallback: &str) -> (String, bool) {
    let candidate = if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw.trim().to_string()
    };
    let mut parts = candidate.rsplitn(2, ':');
    let port_raw = parts.next().unwrap_or("0");
    let host_raw = parts.next().unwrap_or("127.0.0.1");
    let port = port_raw.parse::<u16>().unwrap_or(0);
    let mut host = host_raw.to_string();
    let clamped = if !allow_non_loopback && !is_loopback_host(&host) {
        host = "127.0.0.1".to_string();
        true
    } else {
        false
    };
    (format!("{host}:{port}"), clamped)
}

fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches('[').trim_matches(']');
    if normalized.eq_ignore_ascii_case("localhost") {
        return true;
    }
    normalized
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

fn extract_network_targets(command: &str, args: &[String]) -> Vec<NativeNetworkTarget> {
    let method = infer_network_method(command, args);
    let mut targets = BTreeMap::<String, NativeNetworkTarget>::new();
    for arg in args {
        if let Some(url) = arg.strip_prefix("--url=") {
            add_network_target_from_url(url, &method, &mut targets);
            continue;
        }
        if let Some(url) = arg.strip_prefix("url=") {
            add_network_target_from_url(url, &method, &mut targets);
            continue;
        }
        add_network_target_from_url(arg, &method, &mut targets);
    }
    targets.into_values().collect()
}

fn add_network_target_from_url(
    candidate: &str,
    method: &str,
    targets: &mut BTreeMap<String, NativeNetworkTarget>,
) {
    let Ok(parsed) = reqwest::Url::parse(candidate) else {
        return;
    };
    let Some(host) = parsed.host_str() else {
        return;
    };
    let protocol = parsed.scheme().to_ascii_lowercase();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let target = NativeNetworkTarget {
        protocol,
        host: host.to_ascii_lowercase(),
        port,
        method: method.to_ascii_uppercase(),
    };
    targets.insert(target.cache_key(), target);
}

fn infer_network_method(command: &str, args: &[String]) -> String {
    let lowered = command.trim().to_ascii_lowercase();
    if lowered == "curl" {
        if args.iter().any(|arg| arg.eq_ignore_ascii_case("-I")) {
            return "HEAD".to_string();
        }
        for window in args.windows(2) {
            if window.first().is_some_and(|flag| {
                flag.eq_ignore_ascii_case("-X") || flag.eq_ignore_ascii_case("--request")
            }) {
                return window[1].to_ascii_uppercase();
            }
        }
        return "GET".to_string();
    }
    if lowered == "wget" {
        return "GET".to_string();
    }
    if lowered == "git" {
        return "FETCH".to_string();
    }
    "CONNECT".to_string()
}

fn matches_host_pattern(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let host = host.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    if let Some(suffix) = pattern.strip_prefix('.') {
        return host.ends_with(&format!(".{suffix}"));
    }
    host == pattern
}

fn is_private_or_local_host(host: &str) -> bool {
    let normalized = host.trim().to_ascii_lowercase();
    if normalized == "localhost" {
        return true;
    }
    if normalized
        .rsplit('.')
        .next()
        .is_some_and(|suffix| suffix == "local" || suffix == "internal")
    {
        return true;
    }
    let Ok(ip) = normalized.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4 == Ipv4Addr::UNSPECIFIED
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_unspecified()
        }
    }
}

fn format_network_target_tag_value(target: &NativeNetworkTarget) -> String {
    sanitize_policy_tag_value(&format!(
        "{}://{}:{}@{}",
        target.protocol, target.host, target.port, target.method
    ))
}

fn read_deferred_network_decisions(path: &str) -> Vec<NativeDeferredNetworkDecision> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                let target = value.get("target")?.as_str()?.to_string();
                let decision = value.get("decision")?.as_str()?.to_ascii_lowercase();
                return Some(NativeDeferredNetworkDecision {
                    target_key: target,
                    deny: decision == "deny",
                });
            }
            let mut parts = trimmed.splitn(2, ',');
            let target = parts.next()?.trim().to_string();
            let decision = parts.next()?.trim().to_ascii_lowercase();
            Some(NativeDeferredNetworkDecision {
                target_key: target,
                deny: decision == "deny",
            })
        })
        .collect()
}

fn sanitize_policy_tag_value(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn is_broad_prefix(prefix: &str) -> bool {
    let normalized = prefix.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized == "*"
        || normalized == "."
        || normalized == "/"
        || normalized == "\\"
        || normalized == "~"
        || normalized == "c:"
        || normalized == "c:\\"
    {
        return true;
    }
    if normalized.contains('*') {
        return true;
    }
    matches!(
        normalized.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "cmd" | "cmd.exe" | "powershell" | "pwsh"
    )
}

fn dangerous_command_reason(command: &str, args: &[String]) -> Option<String> {
    let command = command.trim().to_ascii_lowercase();
    let args_joined = args
        .iter()
        .map(|arg| arg.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let has_arg = |target: &str| args.iter().any(|arg| arg.eq_ignore_ascii_case(target));

    if command == "rm"
        && (has_arg("-rf") || has_arg("-fr") || has_arg("-r") && has_arg("-f"))
        && args
            .iter()
            .any(|arg| arg == "/" || arg == "~" || arg == "/*" || arg == "~/*")
    {
        return Some("rm recursive delete targeting root/home".to_string());
    }
    if matches!(
        command.as_str(),
        "sudo" | "dd" | "mkfs" | "shutdown" | "reboot"
    ) {
        return Some(format!("high-risk command '{command}'"));
    }
    if matches!(command.as_str(), "chmod" | "chown") && (has_arg("-r") || has_arg("-R")) {
        return Some(format!("recursive permission mutation via '{command}'"));
    }
    if matches!(command.as_str(), "del" | "erase")
        && (has_arg("/s") || has_arg("/f") || has_arg("/q"))
    {
        return Some("windows recursive delete flags detected".to_string());
    }
    if matches!(command.as_str(), "rmdir" | "rd") && has_arg("/s") && has_arg("/q") {
        return Some("windows recursive directory removal detected".to_string());
    }
    if command == "format" {
        return Some("disk format command detected".to_string());
    }
    if matches!(command.as_str(), "powershell" | "pwsh")
        && args_joined.contains("remove-item")
        && args_joined.contains("-recurse")
        && args_joined.contains("-force")
    {
        return Some("powershell recursive forced delete detected".to_string());
    }
    if matches!(command.as_str(), "cmd" | "cmd.exe")
        && (args_joined.contains("del /f /s /q") || args_joined.contains("rmdir /s /q"))
    {
        return Some("cmd destructive recursive delete detected".to_string());
    }

    None
}

fn matches_command_pattern(pattern: &str, command_line: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    pattern == "*"
        || command_line == pattern
        || command_line.starts_with(&format!("{pattern} "))
        || command_line.starts_with(&format!("{pattern}/"))
}

fn normalize_relative_path(raw: &str, allow_empty: bool) -> Result<PathBuf, NativeToolEngineError> {
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

fn relative_display(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        ".".to_string()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn decode_input<T: DeserializeOwned>(input: &Value) -> Result<T, NativeToolEngineError> {
    let raw = input.to_string();
    let mut deserializer = serde_json::Deserializer::from_str(&raw);
    serde_path_to_error::deserialize::<_, T>(&mut deserializer).map_err(|error| {
        NativeToolEngineError::validation(format!(
            "typed decode failed at '{}': {}",
            error.path(),
            error
        ))
    })
}

fn ensure_can_read(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_read(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "read is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

fn ensure_can_write(scope: Option<&Scope>, rel_path: &Path) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = scope {
        let rel = relative_display(rel_path);
        if !scope.filesystem.can_write(&rel) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "write is outside scope: {rel}"
            )));
        }
    }
    Ok(())
}

fn ensure_repository_read(scope: Option<&Scope>) -> Result<(), NativeToolEngineError> {
    let Some(scope) = scope else {
        return Ok(());
    };
    if scope.repositories.is_empty() {
        return Ok(());
    }
    let readable = scope.repositories.iter().any(|repo| {
        matches!(
            repo.mode,
            crate::core::scope::RepoAccessMode::ReadOnly
                | crate::core::scope::RepoAccessMode::ReadWrite
        )
    });
    if readable {
        Ok(())
    } else {
        Err(NativeToolEngineError::scope_violation(
            "repository read access is not permitted by scope",
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadFileInput {
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadFileOutput {
    path: String,
    content: String,
    byte_size: u64,
}

fn handle_read_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ReadFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_read(ctx.scope, &rel)?;
    let path = ctx.worktree.join(&rel);
    let content = fs::read_to_string(&path).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to read '{}': {error}", path.display()))
    })?;
    let output = ReadFileOutput {
        path: relative_display(&rel),
        byte_size: u64::try_from(content.len()).unwrap_or(u64::MAX),
        content,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode read_file output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    include_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesEntry {
    path: String,
    is_dir: bool,
    byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ListFilesOutput {
    base_path: String,
    entries: Vec<ListFilesEntry>,
}

fn handle_list_files(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<ListFilesInput>(input)?;
    let rel = normalize_relative_path(
        input.path.as_deref().unwrap_or("."),
        true, // allow listing current directory.
    )?;

    if !rel.as_os_str().is_empty() {
        ensure_can_read(ctx.scope, &rel)?;
    }
    let root = ctx.worktree.join(&rel);
    let mut entries = Vec::new();
    list_entries(
        ctx,
        &root,
        &rel,
        input.recursive,
        input.include_hidden,
        &mut entries,
    )?;

    let output = ListFilesOutput {
        base_path: relative_display(&rel),
        entries,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode list_files output: {error}"))
    })
}

fn list_entries(
    ctx: &ToolExecutionContext<'_>,
    absolute_dir: &Path,
    relative_dir: &Path,
    recursive: bool,
    include_hidden: bool,
    entries: &mut Vec<ListFilesEntry>,
) -> Result<(), NativeToolEngineError> {
    let read_dir = fs::read_dir(absolute_dir).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to read directory '{}': {error}",
            absolute_dir.display()
        ))
    })?;

    for item in read_dir {
        let item = item.map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to inspect directory entry in '{}': {error}",
                absolute_dir.display()
            ))
        })?;
        let file_name = item.file_name();
        let name = file_name.to_string_lossy();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let rel_child = if relative_dir.as_os_str().is_empty() {
            PathBuf::from(file_name)
        } else {
            relative_dir.join(file_name)
        };
        if let Some(scope) = ctx.scope {
            let rel_str = relative_display(&rel_child);
            if !scope.filesystem.can_read(&rel_str) {
                continue;
            }
        }

        let metadata = item.metadata().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read metadata for '{}': {error}",
                item.path().display()
            ))
        })?;
        let is_dir = metadata.is_dir();
        let byte_size = if is_dir { 0 } else { metadata.len() };
        entries.push(ListFilesEntry {
            path: relative_display(&rel_child),
            is_dir,
            byte_size,
        });

        if recursive && is_dir {
            list_entries(ctx, &item.path(), &rel_child, true, include_hidden, entries)?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteFileInput {
    path: String,
    content: String,
    #[serde(default)]
    append: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WriteFileOutput {
    path: String,
    bytes_written: u64,
    append: bool,
}

fn handle_write_file(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<WriteFileInput>(input)?;
    let rel = normalize_relative_path(&input.path, false)?;
    ensure_can_write(ctx.scope, &rel)?;

    let absolute = ctx.worktree.join(&rel);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to create parent directory '{}': {error}",
                parent.display()
            ))
        })?;
    }

    let mut options = fs::OpenOptions::new();
    options.create(true).write(true);
    if input.append {
        options.append(true);
    } else {
        options.truncate(true);
    }

    let mut file = options.open(&absolute).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to open '{}': {error}",
            absolute.display()
        ))
    })?;
    file.write_all(input.content.as_bytes()).map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to write '{}': {error}",
            absolute.display()
        ))
    })?;

    let output = WriteFileOutput {
        path: relative_display(&rel),
        bytes_written: u64::try_from(input.content.len()).unwrap_or(u64::MAX),
        append: input.append,
    };
    serde_json::to_value(output).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode write_file output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunCommandInput {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RunCommandOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

#[allow(clippy::too_many_lines)]
fn handle_run_command(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<RunCommandInput>(input)?;
    let command = input.command.trim();
    if command.is_empty() {
        return Err(NativeToolEngineError::validation(
            "run_command requires a non-empty command",
        ));
    }

    let command_line = if input.args.is_empty() {
        command.to_string()
    } else {
        format!("{command} {}", input.args.join(" "))
    };

    if let Some(scope) = ctx.scope {
        if !scope.execution.is_allowed(&command_line) {
            return Err(NativeToolEngineError::scope_violation(format!(
                "run_command blocked by execution scope: {command_line}"
            )));
        }
    }
    let timeout_ms = input
        .timeout_ms
        .unwrap_or(default_timeout_ms)
        .min(default_timeout_ms);
    let network_targets = extract_network_targets(command, &input.args);
    let managed_proxy = if matches!(
        ctx.network_policy.proxy_mode,
        NativeNetworkProxyMode::Managed
    ) {
        Some(ManagedProxyRuntime::start(&ctx.network_policy)?)
    } else {
        None
    };

    let mut cmd = Command::new(command);
    cmd.args(&input.args)
        .current_dir(ctx.worktree)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    if let Some(proxy) = managed_proxy.as_ref() {
        proxy.apply_proxy_env(&mut cmd);
    }
    let mut child = cmd.spawn().map_err(|error| {
        NativeToolEngineError::execution(format!("failed to execute '{command_line}': {error}"))
    })?;

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed while waiting on '{command_line}': {error}"
                ))
            })?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeToolEngineError::timeout("run_command", timeout_ms));
        }
        if matches!(
            ctx.network_policy.approval_mode,
            NativeNetworkApprovalMode::Deferred
        ) && !network_targets.is_empty()
        {
            if let Some(path) = ctx.network_policy.deferred_decisions_file.as_ref() {
                let decisions = read_deferred_network_decisions(path);
                let keys = network_targets
                    .iter()
                    .map(NativeNetworkTarget::cache_key)
                    .collect::<Vec<_>>();

                for decision in decisions {
                    if !keys.iter().any(|key| key == &decision.target_key) {
                        continue;
                    }
                    if decision.deny {
                        let _ = child.kill();
                        let _ = child.wait();
                        let mut tags = ctx.network_policy.base_policy_tags();
                        tags.push("network_approval_outcome:deferred_denied".to_string());
                        tags.push(format!(
                            "network_target:{}",
                            sanitize_policy_tag_value(&decision.target_key)
                        ));
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network deferred denial received for '{}'",
                            decision.target_key
                        ))
                        .with_policy_tags(tags));
                    }
                    let mut cache = ctx.network_approval_cache.borrow_mut();
                    cache.insert_bounded(
                        decision.target_key,
                        ctx.network_policy.approval_cache_max_entries,
                    );
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to collect output for '{command_line}': {error}"
        ))
    })?;

    let status = output.status.code().unwrap_or(-1);
    let result = RunCommandOutput {
        exit_code: status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        timed_out: false,
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode run_command output: {error}"))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NoInput {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitDiffInput {
    #[serde(default)]
    staged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitStatusOutput {
    output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GitDiffOutput {
    output: String,
}

fn handle_git_status(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let _ = decode_input::<NoInput>(input)?;
    ensure_repository_read(ctx.scope)?;
    let mut cmd = Command::new("git");
    cmd.args(["status", "--short", "--branch"])
        .current_dir(ctx.worktree)
        .env_clear();
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    let output = cmd
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git status failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitStatusOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_status output: {error}"))
    })
}

fn handle_git_diff(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GitDiffInput>(input)?;
    ensure_repository_read(ctx.scope)?;

    let mut args = vec!["diff"];
    if input.staged {
        args.push("--staged");
    }
    args.push("--no-ext-diff");

    let mut cmd = Command::new("git");
    cmd.args(&args).current_dir(ctx.worktree).env_clear();
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    let output = cmd
        .output()
        .map_err(|error| NativeToolEngineError::execution(format!("git diff failed: {error}")))?;
    if !output.status.success() {
        return Err(NativeToolEngineError::execution(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let result = GitDiffOutput {
        output: String::from_utf8_lossy(&output.stdout).to_string(),
    };
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode git_diff output: {error}"))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum GraphQueryKindInput {
    Neighbors,
    Dependents,
    Subgraph,
    Filter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GraphQueryInput {
    kind: GraphQueryKindInput,
    #[serde(default)]
    node: Option<String>,
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
    #[serde(default)]
    edge_types: Vec<String>,
    #[serde(default)]
    node_type: Option<String>,
    #[serde(default)]
    path_prefix: Option<String>,
    #[serde(default)]
    partition: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

impl GraphQueryInput {
    fn into_request(self) -> Result<GraphQueryRequest, NativeToolEngineError> {
        match self.kind {
            GraphQueryKindInput::Neighbors => Ok(GraphQueryRequest::Neighbors {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.neighbors requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Dependents => Ok(GraphQueryRequest::Dependents {
                node: self.node.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.dependents requires 'node'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Subgraph => Ok(GraphQueryRequest::Subgraph {
                seed: self.seed.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'seed'")
                })?,
                depth: self.depth.ok_or_else(|| {
                    NativeToolEngineError::validation("graph_query.subgraph requires 'depth'")
                })?,
                edge_types: self.edge_types,
                max_results: self.max_results,
            }),
            GraphQueryKindInput::Filter => Ok(GraphQueryRequest::Filter {
                node_type: self.node_type,
                path_prefix: self.path_prefix,
                partition: self.partition,
                max_results: self.max_results,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphQueryGateError {
    code: String,
    message: String,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotCommit {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotProvenance {
    head_commits: Vec<RuntimeGraphSnapshotCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotRepository {
    repo_name: String,
    repo_path: String,
    commit_hash: String,
    canonical_fingerprint: String,
    document: PortableDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeGraphSnapshotArtifact {
    profile_version: String,
    canonical_fingerprint: String,
    provenance: RuntimeGraphSnapshotProvenance,
    repositories: Vec<RuntimeGraphSnapshotRepository>,
}

fn graph_query_runtime_error(code: &str, message: impl Into<String>) -> NativeToolEngineError {
    NativeToolEngineError::new(code, message, false)
}

fn graph_query_runtime_error_with_refresh_hint(
    code: &str,
    message: impl Into<String>,
) -> NativeToolEngineError {
    graph_query_runtime_error(
        code,
        format!("{}. Hint: {GRAPH_QUERY_REFRESH_HINT}", message.into()),
    )
}

fn resolve_repo_head_commit(
    repo_path: &Path,
    env: &HashMap<String, String>,
) -> Result<String, NativeToolEngineError> {
    let mut cmd = Command::new("git");
    cmd.args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .env_clear();
    for (key, value) in deterministic_env_pairs(env) {
        cmd.env(key, value);
    }
    let output = cmd.output().map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!(
                "failed to resolve git HEAD for '{}': {error}",
                repo_path.display()
            ),
        )
    })?;
    if !output.status.success() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!(
                "failed to resolve git HEAD for '{}': {}",
                repo_path.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let head = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if head.is_empty() {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_stale",
            format!("resolved empty HEAD for '{}'", repo_path.display()),
        ));
    }
    Ok(head)
}

fn graph_query_bounds_from_env(
    env: &HashMap<String, String>,
) -> Result<GraphQueryBounds, NativeToolEngineError> {
    let mut bounds = GraphQueryBounds::default();
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT") {
        bounds.max_results_limit = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_RESULTS_LIMIT '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH") {
        bounds.max_subgraph_depth = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_MAX_SUBGRAPH_DEPTH '{raw}': {error}"
            ))
        })?;
    }
    if let Some(raw) = env.get("HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS") {
        bounds.default_max_results = raw.parse::<usize>().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "invalid HIVEMIND_GRAPH_QUERY_DEFAULT_MAX_RESULTS '{raw}': {error}"
            ))
        })?;
    }
    if bounds.max_results_limit == 0
        || bounds.max_subgraph_depth == 0
        || bounds.default_max_results == 0
    {
        return Err(NativeToolEngineError::execution(
            "graph query bounds configuration values must be > 0",
        ));
    }
    if bounds.default_max_results > bounds.max_results_limit {
        return Err(NativeToolEngineError::execution(format!(
            "graph query default max results {} exceeds max limit {}",
            bounds.default_max_results, bounds.max_results_limit
        )));
    }
    Ok(bounds)
}

fn load_runtime_graph_snapshot(
    env: &HashMap<String, String>,
) -> Result<RuntimeGraphSnapshotArtifact, NativeToolEngineError> {
    if let Some(raw) = env.get(GRAPH_QUERY_ENV_GATE_ERROR) {
        let gate = serde_json::from_str::<RuntimeGraphQueryGateError>(raw).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to decode {GRAPH_QUERY_ENV_GATE_ERROR}: {error}"
            ))
        })?;
        let message = gate.hint.as_ref().map_or_else(
            || gate.message.clone(),
            |hint| format!("{}. Hint: {hint}", gate.message),
        );
        return Err(graph_query_runtime_error(&gate.code, message));
    }

    let snapshot_path = env
        .get(GRAPH_QUERY_ENV_SNAPSHOT_PATH)
        .ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_missing",
                format!("missing runtime env {GRAPH_QUERY_ENV_SNAPSHOT_PATH}"),
            )
        })?
        .clone();
    let raw = fs::read_to_string(&snapshot_path).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("failed to read graph snapshot '{snapshot_path}': {error}"),
        )
    })?;
    if raw.trim().is_empty() || raw.trim() == "{}" {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_missing",
            format!("graph snapshot '{snapshot_path}' is empty"),
        ));
    }

    serde_json::from_str::<RuntimeGraphSnapshotArtifact>(&raw).map_err(|error| {
        graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_invalid",
            format!("invalid graph snapshot '{snapshot_path}': {error}"),
        )
    })
}

fn validate_runtime_graph_snapshot(
    artifact: &RuntimeGraphSnapshotArtifact,
    env: &HashMap<String, String>,
) -> Result<(), NativeToolEngineError> {
    if artifact.profile_version != CODEGRAPH_PROFILE_MARKER {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_profile_mismatch",
            format!(
                "snapshot profile '{}' is not supported; expected '{}'",
                artifact.profile_version, CODEGRAPH_PROFILE_MARKER
            ),
        ));
    }

    let head_by_repo: HashMap<(String, String), String> = artifact
        .provenance
        .head_commits
        .iter()
        .map(|entry| {
            (
                (entry.repo_name.clone(), entry.repo_path.clone()),
                entry.commit_hash.clone(),
            )
        })
        .collect();

    for repo in &artifact.repositories {
        let document = repo.document.to_document().map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "invalid portable document for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        let computed = canonical_fingerprint(&document).map_err(|error| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!(
                    "failed to compute fingerprint for repository '{}': {error}",
                    repo.repo_name
                ),
            )
        })?;
        if computed != repo.canonical_fingerprint {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_integrity_invalid",
                format!("fingerprint mismatch for repository '{}'", repo.repo_name),
            ));
        }

        let key = (repo.repo_name.clone(), repo.repo_path.clone());
        let recorded_head = head_by_repo.get(&key).ok_or_else(|| {
            graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "missing head commit provenance for repository '{}'",
                    repo.repo_name
                ),
            )
        })?;
        if recorded_head != &repo.commit_hash {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "snapshot repository commit mismatch for '{}' (snapshot={} provenance={})",
                    repo.repo_name, repo.commit_hash, recorded_head
                ),
            ));
        }

        let current_head = resolve_repo_head_commit(Path::new(&repo.repo_path), env)?;
        if current_head != *recorded_head {
            return Err(graph_query_runtime_error_with_refresh_hint(
                "graph_snapshot_stale",
                format!(
                    "graph snapshot is stale for repository '{}' (snapshot={} current={})",
                    repo.repo_name, recorded_head, current_head
                ),
            ));
        }
    }

    let aggregate = aggregate_snapshot_fingerprint_registry_style(&artifact.repositories);
    if aggregate != artifact.canonical_fingerprint {
        return Err(graph_query_runtime_error_with_refresh_hint(
            "graph_snapshot_integrity_invalid",
            "aggregate graph snapshot fingerprint mismatch",
        ));
    }

    Ok(())
}

fn aggregate_snapshot_fingerprint_registry_style(
    repositories: &[RuntimeGraphSnapshotRepository],
) -> String {
    let mut entries = repositories
        .iter()
        .map(|repo| {
            format!(
                "{}|{}|{}|{}",
                repo.repo_name, repo.repo_path, repo.commit_hash, repo.canonical_fingerprint
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    let joined = entries.join("\n");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    joined.as_bytes().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn map_graph_query_error(
    error: crate::core::graph_query::GraphQueryError,
) -> NativeToolEngineError {
    NativeToolEngineError::new(error.code, error.message, false)
}

fn handle_graph_query(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<GraphQueryInput>(input)?;
    let request = input.into_request()?;
    let bounds = graph_query_bounds_from_env(ctx.env)?;
    let artifact = load_runtime_graph_snapshot(ctx.env)?;
    validate_runtime_graph_snapshot(&artifact, ctx.env)?;

    let partition_paths = ctx
        .env
        .get(GRAPH_QUERY_ENV_CONSTITUTION_PATH)
        .map(PathBuf::from)
        .as_deref()
        .map_or_else(BTreeMap::new, load_partition_paths_from_constitution);

    let repositories = artifact
        .repositories
        .iter()
        .map(|repo| GraphQueryRepository {
            repo_name: repo.repo_name.clone(),
            document: repo.document.clone(),
        })
        .collect::<Vec<_>>();
    let index = GraphQueryIndex::from_snapshot_repositories(&repositories, &partition_paths)
        .map_err(map_graph_query_error)?;

    let started = Instant::now();
    let mut result = index
        .execute(&request, &artifact.canonical_fingerprint, &bounds)
        .map_err(map_graph_query_error)?;
    result.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode graph_query output: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scope::{ExecutionScope, FilePermission, FilesystemScope, PathRule, Scope};
    use proptest::prelude::*;
    use ucp_api::{
        build_code_graph, CodeGraphBuildInput, CodeGraphExtractorConfig,
        CODEGRAPH_EXTRACTOR_VERSION,
    };

    fn init_git_repo(path: &Path) {
        fs::create_dir_all(path).expect("create repo dir");
        let output = Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("git init");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_commit_all(path: &Path, message: &str) {
        let add = Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .expect("git add");
        assert!(
            add.status.success(),
            "{}",
            String::from_utf8_lossy(&add.stderr)
        );
        let commit = Command::new("git")
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                message,
            ])
            .current_dir(path)
            .output()
            .expect("git commit");
        assert!(
            commit.status.success(),
            "{}",
            String::from_utf8_lossy(&commit.stderr)
        );
    }

    fn git_head(path: &Path) -> String {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .expect("git rev-parse");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write_snapshot_artifact(repo_path: &Path, snapshot_path: &Path) {
        let commit_hash = git_head(repo_path);
        let built = build_code_graph(&CodeGraphBuildInput {
            repository_path: repo_path.to_path_buf(),
            commit_hash: commit_hash.clone(),
            config: CodeGraphExtractorConfig::default(),
        })
        .expect("build code graph");
        let portable = PortableDocument::from_document(&built.document);
        let repositories = vec![RuntimeGraphSnapshotRepository {
            repo_name: "repo".to_string(),
            repo_path: repo_path.to_string_lossy().to_string(),
            commit_hash: commit_hash.clone(),
            canonical_fingerprint: built.canonical_fingerprint.clone(),
            document: portable,
        }];
        let artifact = RuntimeGraphSnapshotArtifact {
            profile_version: CODEGRAPH_PROFILE_MARKER.to_string(),
            canonical_fingerprint: aggregate_snapshot_fingerprint_registry_style(&repositories),
            provenance: RuntimeGraphSnapshotProvenance {
                head_commits: vec![RuntimeGraphSnapshotCommit {
                    repo_name: "repo".to_string(),
                    repo_path: repo_path.to_string_lossy().to_string(),
                    commit_hash,
                }],
            },
            repositories,
        };
        let raw = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": "graph_snapshot.v1",
            "snapshot_version": 1,
            "provenance": {
                "project_id": uuid::Uuid::new_v4(),
                "head_commits": artifact.provenance.head_commits,
                "generated_at": chrono::Utc::now(),
            },
            "ucp_engine_version": CODEGRAPH_EXTRACTOR_VERSION,
            "profile_version": artifact.profile_version,
            "canonical_fingerprint": artifact.canonical_fingerprint,
            "summary": {
                "total_nodes": built.stats.total_nodes,
                "repository_nodes": built.stats.repository_nodes,
                "directory_nodes": built.stats.directory_nodes,
                "file_nodes": built.stats.file_nodes,
                "symbol_nodes": built.stats.symbol_nodes,
                "total_edges": built.stats.total_edges,
                "reference_edges": built.stats.reference_edges,
                "export_edges": built.stats.export_edges,
                "languages": built.stats.languages,
            },
            "repositories": artifact.repositories,
            "static_projection": "",
        }))
        .expect("serialize snapshot");
        fs::write(snapshot_path, raw).expect("write snapshot");
    }

    fn allow_all_scope() -> Scope {
        Scope::new()
            .with_filesystem(
                FilesystemScope::new().with_rule(PathRule::new("*", FilePermission::Write)),
            )
            .with_execution(ExecutionScope::new().allow("*"))
    }

    fn test_tool_context<'a>(
        worktree: &'a Path,
        scope: Option<&'a Scope>,
        policy: &NativeCommandPolicy,
        env: &'a HashMap<String, String>,
    ) -> ToolExecutionContext<'a> {
        test_tool_context_with_policies(
            worktree,
            scope,
            policy,
            env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        )
    }

    fn test_tool_context_with_policies<'a>(
        worktree: &'a Path,
        scope: Option<&'a Scope>,
        policy: &NativeCommandPolicy,
        env: &'a HashMap<String, String>,
        sandbox_policy: NativeSandboxPolicy,
        approval_policy: NativeApprovalPolicy,
        exec_policy_manager: NativeExecPolicyManager,
    ) -> ToolExecutionContext<'a> {
        test_tool_context_with_network_policy(
            worktree,
            scope,
            policy,
            env,
            sandbox_policy,
            approval_policy,
            NativeNetworkPolicy::default(),
            exec_policy_manager,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn test_tool_context_with_network_policy<'a>(
        worktree: &'a Path,
        scope: Option<&'a Scope>,
        policy: &NativeCommandPolicy,
        env: &'a HashMap<String, String>,
        sandbox_policy: NativeSandboxPolicy,
        approval_policy: NativeApprovalPolicy,
        network_policy: NativeNetworkPolicy,
        exec_policy_manager: NativeExecPolicyManager,
    ) -> ToolExecutionContext<'a> {
        ToolExecutionContext {
            worktree,
            scope,
            sandbox_policy,
            approval_policy,
            network_policy,
            command_policy: policy.clone(),
            exec_policy_manager,
            approval_cache: RefCell::new(NativeApprovalCache::default()),
            network_approval_cache: RefCell::new(NativeNetworkApprovalCache::default()),
            env,
        }
    }

    #[test]
    fn rejects_unknown_tool_names() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "nope".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({}),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("unknown tool should fail");
        assert_eq!(error.code, "native_tool_unknown");
    }

    #[test]
    fn rejects_invalid_input_schema() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "read_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "missing": "path" }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("invalid schema should fail");
        assert_eq!(error.code, "native_tool_input_invalid");
    }

    #[test]
    fn write_file_obeys_scope_gate() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src/", FilePermission::Read)),
        );
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "write_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "path": "src/main.rs", "content": "fn main() {}" }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("write should be blocked");
        assert_eq!(error.code, "native_scope_violation");
    }

    #[test]
    fn run_command_is_deny_by_default() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("policy should deny command");
        assert_eq!(error.code, "native_policy_violation");
    }

    #[test]
    fn run_command_respects_allowlist_policy() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let value = engine
            .execute(&action, &ctx)
            .expect("allowlisted command should run");
        let output: RunCommandOutput =
            serde_json::from_value(value).expect("run_command output should decode");
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn run_command_uses_hardened_runtime_env_only() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["sh".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let old = std::env::var("PARENT_SECRET").ok();
        std::env::set_var("PARENT_SECRET", "leak-me");

        let mut env = HashMap::new();
        env.insert("ONLY_THIS".to_string(), "visible".to_string());
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "sh",
                "args": ["-c", "printf '%s|%s' \"$ONLY_THIS\" \"$PARENT_SECRET\""]
            }),
        };

        let value = engine
            .execute(&action, &ctx)
            .expect("allowlisted command should run");
        let output: RunCommandOutput =
            serde_json::from_value(value).expect("run_command output should decode");

        match old {
            Some(value) => std::env::set_var("PARENT_SECRET", value),
            None => std::env::remove_var("PARENT_SECRET"),
        }

        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, "visible|");
    }

    #[test]
    fn sandbox_read_only_denies_write_tool() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let sandbox_policy = NativeSandboxPolicy {
            mode: NativeSandboxMode::ReadOnly,
            ..NativeSandboxPolicy::default()
        };
        let ctx = test_tool_context_with_policies(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            sandbox_policy,
            NativeApprovalPolicy::default(),
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "write_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "path": "src/blocked.txt", "content": "nope" }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("read-only sandbox should deny writes");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "sandbox_mode:read-only"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn approval_on_request_denies_when_review_is_deny() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let env = HashMap::new();
        let scope = allow_all_scope();
        let approval_policy = NativeApprovalPolicy {
            mode: NativeApprovalMode::OnRequest,
            review_decision: NativeApprovalReviewDecision::Deny,
            trusted_prefixes: Vec::new(),
            cache_max_entries: 8,
        };
        let ctx = test_tool_context_with_policies(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            approval_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("on-request with deny decision should block");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "approval_review_decision:deny"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn approval_cache_marks_second_run_as_cached() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let env = HashMap::new();
        let scope = allow_all_scope();
        let approval_policy = NativeApprovalPolicy {
            mode: NativeApprovalMode::OnRequest,
            review_decision: NativeApprovalReviewDecision::Approve,
            trusted_prefixes: Vec::new(),
            cache_max_entries: 8,
        };
        let ctx = test_tool_context_with_policies(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            approval_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "echo", "args": ["hello"] }),
        };

        let first = engine.execute_action_trace("call-1".to_string(), &action, &ctx);
        assert!(first.failure.is_none(), "{first:?}");
        assert!(
            first
                .policy_tags
                .iter()
                .any(|tag| tag == "approval_review_decision:approve"),
            "{:?}",
            first.policy_tags
        );

        let second = engine.execute_action_trace("call-2".to_string(), &action, &ctx);
        assert!(second.failure.is_none(), "{second:?}");
        assert!(
            second
                .policy_tags
                .iter()
                .any(|tag| tag == "approval_review_decision:cached"),
            "{:?}",
            second.policy_tags
        );
    }

    #[test]
    fn dangerous_command_requires_danger_full_access_sandbox() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["rm".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let env = HashMap::new();
        let scope = allow_all_scope();
        let approval_policy = NativeApprovalPolicy {
            mode: NativeApprovalMode::OnRequest,
            review_decision: NativeApprovalReviewDecision::Approve,
            trusted_prefixes: Vec::new(),
            cache_max_entries: 8,
        };
        let sandbox_policy = NativeSandboxPolicy {
            mode: NativeSandboxMode::WorkspaceWrite,
            ..NativeSandboxPolicy::default()
        };
        let ctx = test_tool_context_with_policies(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            sandbox_policy,
            approval_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "command": "rm", "args": ["-rf", "/"] }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("dangerous command must require elevated sandbox");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error.message.contains("dangerous command denied"),
            "{}",
            error.message
        );
    }

    #[test]
    fn approval_denies_broad_command_prefix_rules() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["sh".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let env = HashMap::new();
        let scope = allow_all_scope();
        let approval_policy = NativeApprovalPolicy {
            mode: NativeApprovalMode::OnRequest,
            review_decision: NativeApprovalReviewDecision::Approve,
            trusted_prefixes: Vec::new(),
            cache_max_entries: 8,
        };
        let ctx = test_tool_context_with_policies(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            approval_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "sh",
                "args": ["-c", "echo hello"]
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("broad shell prefix should be rejected");
        assert_eq!(error.code, "native_policy_violation");
        assert!(error.message.contains("broad prefix"), "{}", error.message);
    }

    #[test]
    fn exec_prefix_amendments_are_bounded_and_filter_broad_prefixes() {
        let mut env = HashMap::new();
        env.insert(EXEC_PREFIX_RULE_MAX_ENV_KEY.to_string(), "2".to_string());
        env.insert(
            EXEC_PREFIX_AMENDMENTS_ENV_KEY.to_string(),
            "echo,*,sh,git status".to_string(),
        );
        let manager = NativeExecPolicyManager::from_env(&env);
        assert_eq!(manager.prefix_rule_max, 2);
        assert_eq!(
            manager.prefix_amendments,
            vec!["echo".to_string(), "git status".to_string()]
        );
    }

    #[test]
    fn network_policy_denylist_precedes_allowlist() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            allowlist: vec!["example.com".to_string()],
            denylist: vec!["example.com".to_string()],
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "echo",
                "args": ["https://example.com/resource"]
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("denylist must win over allowlist");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "network_decision:denied_denylist"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn network_policy_blocks_private_host_addresses() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            block_private_addresses: true,
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "echo",
                "args": ["http://127.0.0.1:8080/health"]
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("private host should be blocked");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "network_decision:denied_private_address"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn network_policy_limited_mode_restricts_methods() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            access_mode: NativeNetworkAccessMode::Limited,
            limited_methods: vec!["GET".to_string()],
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "echo",
                "args": ["-X", "POST", "https://example.com/resource"]
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("limited mode should deny unlisted methods");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "network_decision:denied_method_restricted"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn network_immediate_approval_is_cached_for_session() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            approval_mode: NativeNetworkApprovalMode::Immediate,
            approval_decision: NativeNetworkApprovalDecision::Approve,
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "echo",
                "args": ["https://example.com/resource"]
            }),
        };

        let first = engine.execute_action_trace("network-immediate-1".to_string(), &action, &ctx);
        assert!(first.failure.is_none(), "{first:?}");
        assert!(
            first
                .policy_tags
                .iter()
                .any(|tag| tag == "network_approval_outcome:approved_for_session"),
            "{:?}",
            first.policy_tags
        );

        let second = engine.execute_action_trace("network-immediate-2".to_string(), &action, &ctx);
        assert!(second.failure.is_none(), "{second:?}");
        assert!(
            second
                .policy_tags
                .iter()
                .any(|tag| tag == "network_approval_outcome:approved_cached"),
            "{:?}",
            second.policy_tags
        );
    }

    #[test]
    fn deferred_network_denial_terminates_running_command() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let decisions_path = tmp.path().join("network-decisions.log");
        fs::write(&decisions_path, "").expect("seed decisions file");
        let policy = NativeCommandPolicy {
            allowlist: vec!["sh".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            approval_mode: NativeNetworkApprovalMode::Deferred,
            deferred_decisions_file: Some(decisions_path.to_string_lossy().to_string()),
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let writer_path = decisions_path;
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            fs::write(writer_path, "https://example.com:443,deny\n").expect("write denial");
        });

        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "sh",
                "args": ["-c", "sleep 5", "https://example.com"],
                "timeout_ms": 5000
            }),
        };
        let error = engine
            .execute(&action, &ctx)
            .expect_err("deferred denial should terminate command");
        writer.join().expect("writer thread");
        assert_eq!(error.code, "native_policy_violation");
        assert!(
            error
                .policy_tags
                .iter()
                .any(|tag| tag == "network_approval_outcome:deferred_denied"),
            "{:?}",
            error.policy_tags
        );
    }

    #[test]
    fn managed_proxy_bind_is_clamped_without_dangerous_override() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy {
            allowlist: vec!["echo".to_string()],
            denylist: Vec::new(),
            deny_by_default: true,
        };
        let scope = allow_all_scope();
        let env = HashMap::new();
        let network_policy = NativeNetworkPolicy {
            proxy_mode: NativeNetworkProxyMode::Managed,
            proxy_http_bind: "0.0.0.0:0".to_string(),
            proxy_admin_bind: "0.0.0.0:0".to_string(),
            ..NativeNetworkPolicy::default()
        };
        let ctx = test_tool_context_with_network_policy(
            tmp.path(),
            Some(&scope),
            &policy,
            &env,
            NativeSandboxPolicy::default(),
            NativeApprovalPolicy::default(),
            network_policy,
            NativeExecPolicyManager {
                base: policy.clone(),
                ..NativeExecPolicyManager::default()
            },
        );
        let action = NativeToolAction {
            name: "run_command".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "command": "echo",
                "args": ["https://example.com/resource"]
            }),
        };

        let trace = engine.execute_action_trace("managed-proxy-check".to_string(), &action, &ctx);
        assert!(trace.failure.is_none(), "{trace:?}");
        assert!(
            trace
                .policy_tags
                .iter()
                .any(|tag| tag == "network_proxy_bind_clamped:true"),
            "{:?}",
            trace.policy_tags
        );
    }

    #[test]
    fn graph_query_tool_reads_snapshot_with_bounds() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        init_git_repo(&repo);
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
        fs::write(repo.join("src/main.rs"), "fn main() { helper(); }\n").expect("write main");
        git_commit_all(&repo, "seed");

        let snapshot_path = tmp.path().join("graph_snapshot.json");
        write_snapshot_artifact(&repo, &snapshot_path);

        let constitution_path = tmp.path().join("constitution.yaml");
        fs::write(
            &constitution_path,
            "partitions:\n  - id: core\n    path: src\n",
        )
        .expect("write constitution");

        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let mut env = HashMap::new();
        env.insert(
            GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
            snapshot_path.to_string_lossy().to_string(),
        );
        env.insert(
            GRAPH_QUERY_ENV_CONSTITUTION_PATH.to_string(),
            constitution_path.to_string_lossy().to_string(),
        );
        let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "graph_query".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "kind": "filter",
                "node_type": "file",
                "path_prefix": "src",
                "max_results": 50
            }),
        };

        let value = engine
            .execute(&action, &ctx)
            .expect("graph query should run");
        let result: GraphQueryResult = serde_json::from_value(value).expect("decode result");
        assert_eq!(result.query_kind, "filter");
        assert!(
            !result.nodes.is_empty(),
            "{}",
            serde_json::to_string(&result).unwrap_or_default()
        );
        assert!(result.duration_ms <= 5_000);
    }

    #[test]
    fn graph_query_tool_fails_when_snapshot_is_stale() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path().join("repo");
        init_git_repo(&repo);
        fs::create_dir_all(repo.join("src")).expect("mkdir src");
        fs::write(repo.join("src/lib.rs"), "pub fn helper() {}\n").expect("write lib");
        git_commit_all(&repo, "seed");

        let snapshot_path = tmp.path().join("graph_snapshot.json");
        write_snapshot_artifact(&repo, &snapshot_path);

        fs::write(
            repo.join("src/lib.rs"),
            "pub fn helper() { println!(\"x\"); }\n",
        )
        .expect("mutate repo");
        git_commit_all(&repo, "stale");

        let policy = NativeCommandPolicy::default();
        let scope = allow_all_scope();
        let mut env = HashMap::new();
        env.insert(
            GRAPH_QUERY_ENV_SNAPSHOT_PATH.to_string(),
            snapshot_path.to_string_lossy().to_string(),
        );
        let ctx = test_tool_context(repo.as_path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "graph_query".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({
                "kind": "filter",
                "max_results": 10
            }),
        };

        let error = engine
            .execute(&action, &ctx)
            .expect_err("stale snapshot should fail");
        assert_eq!(error.code, "graph_snapshot_stale");
        assert!(error.message.contains("hivemind graph snapshot refresh"));
    }

    proptest! {
        #[test]
        fn replay_is_deterministic_for_write_then_read(
            file_name in "[a-z]{1,8}",
            content in "[ -~]{0,64}"
        ) {
            let engine = NativeToolEngine::default();
            let scope = allow_all_scope();
            let policy = NativeCommandPolicy {
                allowlist: vec!["echo".to_string()],
                denylist: vec!["rm".to_string()],
                deny_by_default: true,
            };

            let run_once = |root: &Path| -> (Value, Value) {
                let env = HashMap::new();
                let ctx = test_tool_context(root, Some(&scope), &policy, &env);
                let relative_path = format!("src/{file_name}.txt");
                let write = NativeToolAction {
                    name: "write_file".to_string(),
                    version: TOOL_VERSION_V1.to_string(),
                    input: json!({"path": relative_path, "content": content}),
                };
                let read = NativeToolAction {
                    name: "read_file".to_string(),
                    version: TOOL_VERSION_V1.to_string(),
                    input: json!({"path": format!("src/{file_name}.txt")}),
                };
                let write_out = engine.execute(&write, &ctx).expect("write must pass");
                let read_out = engine.execute(&read, &ctx).expect("read must pass");
                (write_out, read_out)
            };

            let tmp_a = tempfile::tempdir().expect("tempdir");
            let tmp_b = tempfile::tempdir().expect("tempdir");
            let first = run_once(tmp_a.path());
            let second = run_once(tmp_b.path());
            prop_assert_eq!(first, second);
        }
    }

    #[test]
    fn dispatch_overhead_baseline_is_bounded() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join("README.md"), "hello").expect("seed file");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "list_files".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "path": ".", "recursive": false }),
        };

        let samples = 200_u32;
        let started = Instant::now();
        for _ in 0..samples {
            let _ = engine
                .execute(&action, &ctx)
                .expect("dispatch should succeed");
        }
        let avg_us = started.elapsed().as_micros() / u128::from(samples);
        assert!(
            avg_us < 50_000,
            "dispatch baseline too slow: average {avg_us}us"
        );
    }

    #[test]
    fn validation_latency_baseline_is_bounded() {
        let engine = NativeToolEngine::default();
        let tmp = tempfile::tempdir().expect("tempdir");
        let policy = NativeCommandPolicy::default();
        let env = HashMap::new();
        let scope = allow_all_scope();
        let ctx = test_tool_context(tmp.path(), Some(&scope), &policy, &env);
        let action = NativeToolAction {
            name: "read_file".to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input: json!({ "oops": "invalid" }),
        };

        let samples = 500_u32;
        let started = Instant::now();
        for _ in 0..samples {
            let error = engine
                .execute(&action, &ctx)
                .expect_err("invalid payload should fail");
            assert_eq!(error.code, "native_tool_input_invalid");
        }
        let avg_us = started.elapsed().as_micros() / u128::from(samples);
        assert!(
            avg_us < 20_000,
            "validation baseline too slow: average {avg_us}us"
        );
    }
}
