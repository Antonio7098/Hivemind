use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasher;

const ENV_POLICY_INHERIT_MODE_KEY: &str = "HIVEMIND_RUNTIME_ENV_INHERIT";
const ENV_POLICY_INHERIT_ALL: &str = "all";
const ENV_POLICY_INHERIT_CORE: &str = "core";
const ENV_POLICY_INHERIT_NONE: &str = "none";
const ENV_POLICY_INTERNAL_RESERVED_PREFIXES: [&str; 7] = [
    "HIVEMIND_TASK_",
    "HIVEMIND_ATTEMPT_",
    "HIVEMIND_FLOW_",
    "HIVEMIND_REPO_",
    "HIVEMIND_GRAPH_",
    "HIVEMIND_SCOPE_TRACE_",
    "HIVEMIND_RUNTIME_INTERNAL_",
];
const ENV_POLICY_INTERNAL_RESERVED_KEYS: [&str; 6] = [
    "HIVEMIND_PRIMARY_WORKTREE",
    "HIVEMIND_ALL_WORKTREES",
    "HIVEMIND_TASK_SCOPE_JSON",
    "HIVEMIND_BIN",
    "HIVEMIND_AGENT_BIN",
    "HIVEMIND_PROJECT_CONSTITUTION_PATH",
];
const ENV_POLICY_CORE_INHERITED_KEYS: [&str; 18] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SYSTEMROOT",
    "COMSPEC",
    "PATHEXT",
    "WINDIR",
    "NUMBER_OF_PROCESSORS",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEnvInheritMode {
    All,
    Core,
    None,
}

impl RuntimeEnvInheritMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => ENV_POLICY_INHERIT_ALL,
            Self::Core => ENV_POLICY_INHERIT_CORE,
            Self::None => ENV_POLICY_INHERIT_NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEnvBuildError {
    pub code: String,
    pub message: String,
}

impl RuntimeEnvBuildError {
    #[must_use]
    pub fn invalid_inherit_mode(raw: &str) -> Self {
        Self {
            code: "runtime_env_inherit_mode_invalid".to_string(),
            message: format!(
                "Invalid {ENV_POLICY_INHERIT_MODE_KEY} value '{raw}'. Expected one of: {ENV_POLICY_INHERIT_ALL}, {ENV_POLICY_INHERIT_CORE}, {ENV_POLICY_INHERIT_NONE}"
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEnvironmentProvenance {
    pub inherit_mode: RuntimeEnvInheritMode,
    #[serde(default)]
    pub inherited_keys: Vec<String>,
    #[serde(default)]
    pub overlay_keys: Vec<String>,
    #[serde(default)]
    pub explicit_sensitive_overlay_keys: Vec<String>,
    #[serde(default)]
    pub dropped_sensitive_inherited_keys: Vec<String>,
    #[serde(default)]
    pub dropped_reserved_inherited_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedRuntimeEnvironment {
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub provenance: RuntimeEnvironmentProvenance,
}

fn parse_inherit_mode<S: BuildHasher>(
    runtime_overlay: &HashMap<String, String, S>,
) -> std::result::Result<RuntimeEnvInheritMode, RuntimeEnvBuildError> {
    let Some(raw) = runtime_overlay.get(ENV_POLICY_INHERIT_MODE_KEY) else {
        return Ok(RuntimeEnvInheritMode::Core);
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        ENV_POLICY_INHERIT_ALL => Ok(RuntimeEnvInheritMode::All),
        ENV_POLICY_INHERIT_CORE => Ok(RuntimeEnvInheritMode::Core),
        ENV_POLICY_INHERIT_NONE => Ok(RuntimeEnvInheritMode::None),
        _ => Err(RuntimeEnvBuildError::invalid_inherit_mode(raw)),
    }
}

fn is_core_inherited_key(key: &str) -> bool {
    ENV_POLICY_CORE_INHERITED_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    upper.contains("KEY") || upper.contains("SECRET") || upper.contains("TOKEN")
}

fn is_reserved_internal_key(key: &str) -> bool {
    ENV_POLICY_INTERNAL_RESERVED_KEYS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(key))
        || ENV_POLICY_INTERNAL_RESERVED_PREFIXES
            .iter()
            .any(|prefix| key.starts_with(prefix))
}

#[must_use]
pub fn deterministic_env_pairs<S: BuildHasher>(
    env: &HashMap<String, String, S>,
) -> Vec<(String, String)> {
    let mut ordered = BTreeMap::new();
    for (key, value) in env {
        ordered.insert(key.clone(), value.clone());
    }
    ordered.into_iter().collect()
}

pub fn build_protected_runtime_environment<S: BuildHasher>(
    runtime_overlay: &HashMap<String, String, S>,
) -> std::result::Result<ProtectedRuntimeEnvironment, RuntimeEnvBuildError> {
    let inherit_mode = parse_inherit_mode(runtime_overlay)?;
    let inherited_candidates: Vec<(String, String)> = match inherit_mode {
        RuntimeEnvInheritMode::All => std::env::vars().collect(),
        RuntimeEnvInheritMode::Core => std::env::vars()
            .filter(|(key, _)| is_core_inherited_key(key))
            .collect(),
        RuntimeEnvInheritMode::None => Vec::new(),
    };

    let mut ordered_env = BTreeMap::new();
    let mut inherited_keys = Vec::new();
    let mut overlay_keys = Vec::new();
    let mut explicit_sensitive_overlay_keys = Vec::new();
    let mut dropped_sensitive_inherited_keys = Vec::new();
    let mut dropped_reserved_inherited_keys = Vec::new();

    let mut inherited_sorted = inherited_candidates;
    inherited_sorted.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, value) in inherited_sorted {
        if is_reserved_internal_key(&key) {
            dropped_reserved_inherited_keys.push(key);
            continue;
        }
        if is_sensitive_key(&key) {
            dropped_sensitive_inherited_keys.push(key);
            continue;
        }
        inherited_keys.push(key.clone());
        ordered_env.insert(key, value);
    }

    for (key, value) in deterministic_env_pairs(runtime_overlay) {
        if key.eq_ignore_ascii_case(ENV_POLICY_INHERIT_MODE_KEY) {
            continue;
        }
        if is_sensitive_key(&key) {
            explicit_sensitive_overlay_keys.push(key.clone());
        }
        overlay_keys.push(key.clone());
        ordered_env.insert(key, value);
    }

    Ok(ProtectedRuntimeEnvironment {
        env: ordered_env.into_iter().collect(),
        provenance: RuntimeEnvironmentProvenance {
            inherit_mode,
            inherited_keys,
            overlay_keys,
            explicit_sensitive_overlay_keys,
            dropped_sensitive_inherited_keys,
            dropped_reserved_inherited_keys,
        },
    })
}

#[cfg(test)]
pub(super) fn parse_env_inherit_mode_for_test<S: BuildHasher>(
    runtime_overlay: &HashMap<String, String, S>,
) -> std::result::Result<RuntimeEnvInheritMode, RuntimeEnvBuildError> {
    parse_inherit_mode(runtime_overlay)
}

#[cfg(test)]
pub(super) const ENV_POLICY_INHERIT_ALL_FOR_TEST: &str = ENV_POLICY_INHERIT_ALL;
#[cfg(test)]
pub(super) const ENV_POLICY_INHERIT_MODE_KEY_FOR_TEST: &str = ENV_POLICY_INHERIT_MODE_KEY;
#[cfg(test)]
pub(super) const ENV_POLICY_INHERIT_NONE_FOR_TEST: &str = ENV_POLICY_INHERIT_NONE;
