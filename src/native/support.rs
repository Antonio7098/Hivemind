use super::*;
use crate::native::tool_engine::ToolPermission;

pub(crate) const fn default_prompt_headroom() -> usize {
    8_192
}

impl AgentMode {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Freeflow => "freeflow",
            Self::TaskExecutor => "task_executor",
        }
    }

    #[must_use]
    pub(crate) fn allows_permissions(self, permissions: &[ToolPermission]) -> bool {
        match self {
            Self::Planner => permissions.iter().all(|permission| {
                matches!(
                    permission,
                    ToolPermission::FilesystemRead | ToolPermission::GitRead
                )
            }),
            Self::Freeflow | Self::TaskExecutor => true,
        }
    }
}

impl AgentLoopState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Think => "think",
            Self::Act => "act",
            Self::Done => "done",
        }
    }
}

impl Default for NativeRuntimeConfig {
    fn default() -> Self {
        Self {
            max_turns: 8,
            timeout_budget: Duration::from_secs(300),
            token_budget: 3_000_000,
            prompt_headroom: default_prompt_headroom(),
            agent_mode: AgentMode::TaskExecutor,
            capture_full_payloads: false,
        }
    }
}

impl<T: ModelClient + ?Sized> ModelClient for Box<T> {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        (**self).complete_turn(request)
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        (**self).take_transport_telemetry()
    }
}

impl ModelDirective {
    pub(crate) fn target_state(&self) -> AgentLoopState {
        match self {
            Self::Think { .. } => AgentLoopState::Think,
            Self::Act { .. } => AgentLoopState::Act,
            Self::Done { .. } => AgentLoopState::Done,
        }
    }

    pub(crate) fn parse_relaxed(raw: &str) -> Option<Self> {
        for candidate in directive_candidates(raw) {
            if let Some(directive) = parse_directive_candidate(candidate) {
                return Some(directive);
            }
        }
        infer_freeform_think(raw)
    }
}

fn directive_candidates(raw: &str) -> Vec<&str> {
    let trimmed = raw.trim().trim_matches('`').trim();
    let mut candidates = Vec::new();
    if !trimmed.is_empty() {
        candidates.push(trimmed);
    }
    for line in trimmed.lines() {
        let line = line.trim().trim_matches('`').trim();
        if !line.is_empty() {
            candidates.push(line);
        }
    }
    candidates
}

fn parse_directive_candidate(candidate: &str) -> Option<ModelDirective> {
    if let Some(directive) = parse_directive_prefix(strip_leading_formatting(candidate)) {
        return Some(directive);
    }
    find_embedded_directive(candidate).and_then(parse_directive_prefix)
}

fn parse_directive_prefix(candidate: &str) -> Option<ModelDirective> {
    let candidate = candidate.trim();
    for kind in ["THINK", "ACT", "DONE"] {
        let Some(rest) = candidate.get(kind.len()..) else {
            continue;
        };
        if !candidate[..kind.len()].eq_ignore_ascii_case(kind) {
            continue;
        }
        let normalized = if let Some(rest) = rest.trim_start().strip_prefix(':') {
            rest.trim()
        } else if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            rest.trim()
        } else {
            continue;
        };
        return Some(match kind {
            "THINK" => ModelDirective::Think {
                message: normalized.to_string(),
            },
            "ACT" => ModelDirective::Act {
                action: normalized.to_string(),
            },
            "DONE" => ModelDirective::Done {
                summary: normalized.to_string(),
            },
            _ => unreachable!(),
        });
    }
    None
}

fn strip_leading_formatting(candidate: &str) -> &str {
    let mut current = candidate.trim();
    loop {
        let next = if let Some(rest) = current.strip_prefix("- ") {
            rest
        } else if let Some(rest) = current.strip_prefix("* ") {
            rest
        } else if let Some(rest) = current.strip_prefix("• ") {
            rest
        } else {
            strip_numbering_prefix(current).unwrap_or(current)
        };
        if next == current {
            return current.trim();
        }
        current = next.trim_start();
    }
}

fn strip_numbering_prefix(candidate: &str) -> Option<&str> {
    let digit_count = candidate
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .count();
    if digit_count == 0 {
        return None;
    }
    let rest = candidate.get(digit_count..)?;
    if let Some(rest) = rest.strip_prefix('.') {
        return Some(rest.trim_start());
    }
    if let Some(rest) = rest.strip_prefix(')') {
        return Some(rest.trim_start());
    }
    None
}

fn find_embedded_directive(candidate: &str) -> Option<&str> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let mut best: Option<usize> = None;
    for token in ["think:", "act:", "done:", "think ", "act ", "done "] {
        if let Some(index) = lower.find(token) {
            let boundary_ok = index == 0
                || trimmed[..index]
                    .chars()
                    .last()
                    .is_some_and(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
            if boundary_ok && best.is_none_or(|current| index < current) {
                best = Some(index);
            }
        }
    }
    best.and_then(|index| trimmed.get(index..))
}

fn infer_freeform_think(raw: &str) -> Option<ModelDirective> {
    let trimmed = raw.trim().trim_matches('`').trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    let first_person_or_planning = [
        "i ", "i'", "let me", "first ", "next ", "i will", "i'll", "plan:",
    ]
    .iter()
    .any(|prefix| lowered.starts_with(prefix));
    if !first_person_or_planning {
        return None;
    }
    Some(ModelDirective::Think {
        message: trimmed.lines().next().unwrap_or(trimmed).trim().to_string(),
    })
}
