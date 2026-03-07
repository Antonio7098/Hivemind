use super::*;

impl AppState {
    /// Creates a new empty state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an event to the state, returning a new state.
    #[must_use]
    pub fn apply(mut self, event: &Event) -> Self {
        self.apply_mut(event);
        self
    }
}
