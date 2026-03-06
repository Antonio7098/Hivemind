use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadinessState {
    Pending,
    Ready,
    Failed,
}

impl ReadinessState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
struct ReadinessEntry {
    token: String,
    state: ReadinessState,
}

#[derive(Debug, Clone)]
pub(super) struct NativeReadinessGate {
    components: BTreeMap<String, ReadinessEntry>,
    transitions: Vec<NativeReadinessTransition>,
}

impl NativeReadinessGate {
    pub(super) fn new() -> Self {
        Self {
            components: BTreeMap::new(),
            transitions: Vec::new(),
        }
    }

    pub(super) fn register(&mut self, component: &str) -> String {
        let token = format!("{component}:{}", Uuid::new_v4());
        self.components.insert(
            component.to_string(),
            ReadinessEntry {
                token: token.clone(),
                state: ReadinessState::Pending,
            },
        );
        token
    }

    pub(super) fn mark_ready(&mut self, token: &str, reason: impl Into<String>) {
        self.transition(token, ReadinessState::Ready, reason.into());
    }

    pub(super) fn mark_failed(&mut self, token: &str, reason: impl Into<String>) {
        self.transition(token, ReadinessState::Failed, reason.into());
    }

    fn transition(&mut self, token: &str, target: ReadinessState, reason: String) {
        let Some((component_name, entry)) = self
            .components
            .iter_mut()
            .find(|(_, candidate)| candidate.token == token)
        else {
            return;
        };
        let from_state = entry.state;
        entry.state = target;
        self.transitions.push(NativeReadinessTransition {
            component: component_name.clone(),
            token: token.to_string(),
            from_state: from_state.as_str().to_string(),
            to_state: target.as_str().to_string(),
            reason,
            timestamp_ms: now_ms(),
        });
    }

    pub(super) fn all_ready(&self) -> bool {
        self.components
            .values()
            .all(|entry| entry.state == ReadinessState::Ready)
    }

    pub(super) fn transitions(&self) -> Vec<NativeReadinessTransition> {
        self.transitions.clone()
    }
}
