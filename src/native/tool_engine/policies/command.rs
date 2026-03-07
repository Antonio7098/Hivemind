use super::*;

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

    pub(crate) fn is_allowed(
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
