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
use std::env;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, OnceLock};
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

fn terminate_child_process_group(child: &mut Child) {
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(child.id()) else {
            let _ = child.kill();
            let _ = child.wait();
            return;
        };
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .status();
        thread::sleep(Duration::from_millis(20));
        if child.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill")
                .args(["-KILL", &format!("-{pid}")])
                .status();
        }
    }

    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
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
        engine.register_builtin::<ExecCommandInput, ExecSessionOutput>(
            "exec_command",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_exec_command,
        )?;
        engine.register_builtin::<WriteStdinInput, ExecSessionOutput>(
            "write_stdin",
            "execution",
            vec![ToolPermission::Execution],
            DEFAULT_TIMEOUT_MS,
            true,
            handle_write_stdin,
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
        evaluate_tool_policies_impl(action, tool, ctx)
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

mod helpers;
use helpers::*;

mod exec_sessions;
pub use exec_sessions::cleanup_exec_sessions;
use exec_sessions::*;

mod graph_query_tool;
use graph_query_tool::*;

mod policy_eval;
use policy_eval::evaluate_tool_policies_impl;

mod run_command_tool;
use run_command_tool::*;

mod filesystem_tools;
use filesystem_tools::*;

mod git_tools;
use git_tools::*;

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

#[cfg(test)]
mod tests;
