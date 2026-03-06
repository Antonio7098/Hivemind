use super::*;

impl RepositoryScope {
    /// Creates a new repository scope.
    #[must_use]
    pub fn new(repo: impl Into<String>, mode: RepoAccessMode) -> Self {
        Self {
            repo: repo.into(),
            mode,
        }
    }

    /// Creates a read-only repository scope.
    #[must_use]
    pub fn read_only(repo: impl Into<String>) -> Self {
        Self::new(repo, RepoAccessMode::ReadOnly)
    }

    /// Creates a read-write repository scope.
    #[must_use]
    pub fn read_write(repo: impl Into<String>) -> Self {
        Self::new(repo, RepoAccessMode::ReadWrite)
    }
}
