use super::*;

/// Deterministic scripted model implementation for tests and harnessing.
#[derive(Debug, Clone, Default)]
pub struct MockModelClient {
    scripted: VecDeque<Result<String, NativeRuntimeError>>,
}

impl MockModelClient {
    #[must_use]
    pub fn new(scripted: Vec<Result<String, NativeRuntimeError>>) -> Self {
        Self {
            scripted: VecDeque::from(scripted),
        }
    }

    #[must_use]
    pub fn from_outputs(scripted: Vec<String>) -> Self {
        let turns = scripted.into_iter().map(Ok).collect::<Vec<_>>();
        Self::new(turns)
    }

    #[must_use]
    pub fn deterministic_default() -> Self {
        Self::from_outputs(vec![
            "ACT:emit deterministic runtime marker".to_string(),
            "DONE:native runtime completed deterministically".to_string(),
        ])
    }
}

impl ModelClient for MockModelClient {
    fn complete_turn(&mut self, _request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        self.scripted.pop_front().unwrap_or_else(|| {
            Ok("DONE:mock model exhausted scripted outputs; auto-completing".to_string())
        })
    }
}
