//! Native typed tool engine with schema validation and policy-aware dispatch.
//!
//! Sprint 44 introduces a deterministic tool registry for native runtime turns.

use crate::adapters::runtime::{
    deterministic_env_pairs, NativeToolCallFailure, NativeToolCallTrace,
};
use crate::core::graph_query::{
    load_partition_paths_from_constitution, GraphQueryBounds, GraphQueryIndex,
    GraphQueryRepository, GraphQueryRequest, GraphQueryResult, GRAPH_QUERY_ENV_CONSTITUTION_PATH,
    GRAPH_QUERY_ENV_GATE_ERROR, GRAPH_QUERY_ENV_PROJECT_ID, GRAPH_QUERY_ENV_SNAPSHOT_PATH,
    GRAPH_QUERY_REFRESH_HINT,
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
const CHECKPOINT_COMPLETE_TIMEOUT_MS: u64 = 20_000;
const GRAPH_QUERY_TIMEOUT_MS: u64 = 15_000;
const TOOL_TRACE_RESPONSE_MAX_CHARS: usize = 12_000;

/// Structured native tool engine error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeToolEngineError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    #[serde(default)]
    pub policy_tags: Vec<String>,
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
    #[serde(default = "action::default_tool_version")]
    pub version: String,
    #[serde(default)]
    pub input: Value,
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

mod policies;
use policies::{
    matches_command_pattern, normalize_relative_path, relative_display,
    NativeDeferredNetworkDecision, NativeNetworkTarget,
};
pub use policies::{
    NativeApprovalCache, NativeApprovalMode, NativeApprovalPolicy, NativeApprovalReviewDecision,
    NativeCommandPolicy, NativeExecPolicyManager, NativeNetworkAccessMode,
    NativeNetworkApprovalCache, NativeNetworkApprovalDecision, NativeNetworkApprovalMode,
    NativeNetworkPolicy, NativeNetworkProxyMode, NativeSandboxMode, NativeSandboxPolicy,
    ToolExecutionContext,
};

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

mod action;
mod engine;
mod error;

mod helpers;
use helpers::*;

mod exec_sessions;
pub use exec_sessions::cleanup_exec_sessions;
use exec_sessions::*;

mod graph_query_tool;
pub(crate) use graph_query_tool::mark_runtime_graph_registry_dirty;
#[cfg(test)]
pub(crate) use graph_query_tool::{
    aggregate_snapshot_fingerprint_registry_style, RuntimeGraphSnapshotArtifact,
    RuntimeGraphSnapshotCommit, RuntimeGraphSnapshotProvenance, RuntimeGraphSnapshotRepository,
};
use graph_query_tool::{handle_graph_query, GraphQueryInput};

mod policy_eval;
use policy_eval::evaluate_tool_policies_impl;

mod run_command_tool;
use run_command_tool::*;

mod checkpoint_tool;
use checkpoint_tool::*;

mod filesystem_tools;
use filesystem_tools::*;

mod git_tools;
use git_tools::*;

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
