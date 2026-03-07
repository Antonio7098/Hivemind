//! Scope model for defining capability contracts.
//!
//! Scope is an explicit, enforceable capability contract that defines
//! what operations are permitted during task execution.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

mod compatibility;
mod execution;
mod filesystem;
mod git;
mod repository;

#[cfg(test)]
mod tests;

pub use compatibility::check_compatibility;

/// Filesystem permission level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilePermission {
    /// Read-only access.
    Read,
    /// Read and write access.
    Write,
    /// Explicitly denied access.
    Deny,
}

/// A filesystem path rule within a scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathRule {
    /// The path pattern (may include globs).
    pub pattern: String,
    /// The permission level for this path.
    pub permission: FilePermission,
}

/// Filesystem scope defining allowed paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemScope {
    /// Path rules (evaluated in order, first match wins).
    pub rules: Vec<PathRule>,
}

/// Repository access mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoAccessMode {
    /// Read-only access.
    ReadOnly,
    /// Read and write access.
    #[default]
    ReadWrite,
}

/// Repository scope defining repository access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryScope {
    /// Repository name or path.
    pub repo: String,
    /// Access mode.
    pub mode: RepoAccessMode,
}

/// Git operation permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitPermission {
    /// May create commits.
    Commit,
    /// May create branches.
    Branch,
    /// May push to remote.
    Push,
}

/// Git scope defining git operation permissions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitScope {
    /// Allowed git operations.
    pub permissions: HashSet<GitPermission>,
}

/// Execution scope defining allowed commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionScope {
    /// Allowed command patterns.
    pub allowed: Vec<String>,
    /// Denied command patterns.
    pub denied: Vec<String>,
}

/// Complete scope contract for a task.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    /// Filesystem access rules.
    pub filesystem: FilesystemScope,
    /// Repository access rules.
    pub repositories: Vec<RepositoryScope>,
    /// Git operation permissions.
    pub git: GitScope,
    /// Command execution permissions.
    pub execution: ExecutionScope,
}

impl Scope {
    /// Creates an empty scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the filesystem scope.
    #[must_use]
    pub fn with_filesystem(mut self, fs: FilesystemScope) -> Self {
        self.filesystem = fs;
        self
    }

    /// Adds a repository scope.
    #[must_use]
    pub fn with_repository(mut self, repo: RepositoryScope) -> Self {
        self.repositories.push(repo);
        self
    }

    /// Sets the git scope.
    #[must_use]
    pub fn with_git(mut self, git: GitScope) -> Self {
        self.git = git;
        self
    }

    /// Sets the execution scope.
    #[must_use]
    pub fn with_execution(mut self, exec: ExecutionScope) -> Self {
        self.execution = exec;
        self
    }
}

/// Result of scope compatibility check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeCompatibility {
    /// Scopes are compatible - safe to run in parallel.
    Compatible,
    /// Soft conflict - potentially unsafe, may need isolation.
    SoftConflict,
    /// Hard conflict - unsafe, must isolate or serialize.
    HardConflict,
}
