use super::*;

impl ExecutionScope {
    /// Creates an empty execution scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows a command pattern.
    #[must_use]
    pub fn allow(mut self, pattern: impl Into<String>) -> Self {
        self.allowed.push(pattern.into());
        self
    }

    /// Denies a command pattern.
    #[must_use]
    pub fn deny(mut self, pattern: impl Into<String>) -> Self {
        self.denied.push(pattern.into());
        self
    }

    /// Checks if a command is allowed.
    #[must_use]
    pub fn is_allowed(&self, command: &str) -> bool {
        for pattern in &self.denied {
            if command.starts_with(pattern) || pattern == "*" {
                return false;
            }
        }
        if self.allowed.is_empty() {
            return true;
        }
        for pattern in &self.allowed {
            if command.starts_with(pattern) || pattern == "*" {
                return true;
            }
        }
        false
    }
}
