use super::*;

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
    pub(crate) fn contains(&self, key: &str) -> bool {
        self.approved_for_session.iter().any(|item| item == key)
    }

    pub(crate) fn matches_command_line(&self, command_line: &str) -> bool {
        self.approved_for_session
            .iter()
            .any(|pattern| matches_command_pattern(pattern, command_line))
    }

    pub(crate) fn insert_bounded(&mut self, key: String, max_entries: usize) {
        self.approved_for_session.retain(|item| item != &key);
        self.approved_for_session.push_back(key);
        while self.approved_for_session.len() > max_entries {
            let _ = self.approved_for_session.pop_front();
        }
    }
}
