use super::*;

impl GitScope {
    /// Creates an empty git scope (read-only).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a git scope that allows commits.
    #[must_use]
    pub fn with_commit() -> Self {
        let mut scope = Self::new();
        scope.permissions.insert(GitPermission::Commit);
        scope
    }

    /// Creates a git scope that allows commits and branches.
    #[must_use]
    pub fn with_branch() -> Self {
        let mut scope = Self::with_commit();
        scope.permissions.insert(GitPermission::Branch);
        scope
    }

    /// Checks if commits are allowed.
    #[must_use]
    pub fn can_commit(&self) -> bool {
        self.permissions.contains(&GitPermission::Commit)
    }

    /// Checks if branching is allowed.
    #[must_use]
    pub fn can_branch(&self) -> bool {
        self.permissions.contains(&GitPermission::Branch)
    }
}
