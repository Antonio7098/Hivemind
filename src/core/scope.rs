//! Scope model for defining capability contracts.
//!
//! Scope is an explicit, enforceable capability contract that defines
//! what operations are permitted during task execution.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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

impl PathRule {
    /// Creates a new path rule.
    #[must_use]
    pub fn new(pattern: impl Into<String>, permission: FilePermission) -> Self {
        Self {
            pattern: pattern.into(),
            permission,
        }
    }

    /// Creates a read-only path rule.
    #[must_use]
    pub fn read(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Read)
    }

    /// Creates a read-write path rule.
    #[must_use]
    pub fn write(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Write)
    }

    /// Creates a deny path rule.
    #[must_use]
    pub fn deny(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Deny)
    }

    /// Checks if this rule matches a given path.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        if self.pattern.contains('*') {
            glob_match(&self.pattern, path)
        } else {
            path.starts_with(&self.pattern)
        }
    }
}

/// Simple glob matching (supports * and **).
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == "*" {
        return true;
    }
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');
            // Check prefix
            if !prefix.is_empty() && !path.starts_with(prefix) {
                return false;
            }
            // Check suffix (e.g., *.rs)
            if !suffix.is_empty() {
                if let Some(ext) = suffix.strip_prefix("*.") {
                    return path.ends_with(&format!(".{ext}"));
                }
                return path.ends_with(suffix);
            }
            return true;
        }
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix("*/") {
        return path.ends_with(suffix);
    }
    pattern == path
}

/// Filesystem scope defining allowed paths.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesystemScope {
    /// Path rules (evaluated in order, first match wins).
    pub rules: Vec<PathRule>,
}

impl FilesystemScope {
    /// Creates an empty filesystem scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a path rule.
    #[must_use]
    pub fn with_rule(mut self, rule: PathRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Gets the permission for a path.
    #[must_use]
    pub fn permission_for(&self, path: &str) -> Option<FilePermission> {
        let mut has_read = false;
        let mut has_write = false;

        for rule in &self.rules {
            if !rule.matches(path) {
                continue;
            }

            match rule.permission {
                FilePermission::Deny => return Some(FilePermission::Deny),
                FilePermission::Write => has_write = true,
                FilePermission::Read => has_read = true,
            }
        }

        if has_write {
            Some(FilePermission::Write)
        } else if has_read {
            Some(FilePermission::Read)
        } else {
            None
        }
    }

    /// Checks if a path is allowed for writing.
    #[must_use]
    pub fn can_write(&self, path: &str) -> bool {
        matches!(self.permission_for(path), Some(FilePermission::Write))
    }

    /// Checks if a path is allowed for reading.
    #[must_use]
    pub fn can_read(&self, path: &str) -> bool {
        matches!(
            self.permission_for(path),
            Some(FilePermission::Read | FilePermission::Write)
        )
    }
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

/// Execution scope defining allowed commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionScope {
    /// Allowed command patterns.
    pub allowed: Vec<String>,
    /// Denied command patterns.
    pub denied: Vec<String>,
}

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
        // Deny rules take precedence
        for pattern in &self.denied {
            if command.starts_with(pattern) || pattern == "*" {
                return false;
            }
        }
        // Check allow rules
        if self.allowed.is_empty() {
            return true; // No restrictions
        }
        for pattern in &self.allowed {
            if command.starts_with(pattern) || pattern == "*" {
                return true;
            }
        }
        false
    }
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

impl ScopeCompatibility {
    /// Combines two compatibility results (worst case wins).
    #[must_use]
    pub fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::HardConflict, _) | (_, Self::HardConflict) => Self::HardConflict,
            (Self::SoftConflict, _) | (_, Self::SoftConflict) => Self::SoftConflict,
            _ => Self::Compatible,
        }
    }
}

/// Checks compatibility between two scopes.
#[must_use]
pub fn check_compatibility(a: &Scope, b: &Scope) -> ScopeCompatibility {
    let fs_compat = check_filesystem_compatibility(&a.filesystem, &b.filesystem);
    let repo_compat = check_repository_compatibility(&a.repositories, &b.repositories);
    let git_compat = check_git_compatibility(&a.git, &b.git);
    let exec_compat = check_execution_compatibility(&a.execution, &b.execution);

    fs_compat
        .combine(repo_compat)
        .combine(git_compat)
        .combine(exec_compat)
}

fn check_filesystem_compatibility(a: &FilesystemScope, b: &FilesystemScope) -> ScopeCompatibility {
    // Check for overlapping write paths
    for rule_a in &a.rules {
        for rule_b in &b.rules {
            // Check if patterns might overlap
            if patterns_might_overlap(&rule_a.pattern, &rule_b.pattern) {
                // Deny rules always produce a hard conflict when overlapping.
                if rule_a.permission == FilePermission::Deny
                    || rule_b.permission == FilePermission::Deny
                {
                    return ScopeCompatibility::HardConflict;
                }
                match (rule_a.permission, rule_b.permission) {
                    (FilePermission::Write, FilePermission::Write) => {
                        return ScopeCompatibility::HardConflict;
                    }
                    (FilePermission::Read, FilePermission::Write)
                    | (FilePermission::Write, FilePermission::Read) => {
                        return ScopeCompatibility::SoftConflict;
                    }
                    _ => {}
                }
            }
        }
    }
    ScopeCompatibility::Compatible
}

fn patterns_might_overlap(a: &str, b: &str) -> bool {
    // Conservative check - if either contains glob, assume possible overlap
    if a.contains('*') || b.contains('*') {
        return true;
    }
    // Check if one is prefix of the other
    a.starts_with(b) || b.starts_with(a)
}

fn check_repository_compatibility(
    a: &[RepositoryScope],
    b: &[RepositoryScope],
) -> ScopeCompatibility {
    for repo_a in a {
        for repo_b in b {
            if repo_a.repo == repo_b.repo {
                match (repo_a.mode, repo_b.mode) {
                    (RepoAccessMode::ReadWrite, RepoAccessMode::ReadWrite) => {
                        return ScopeCompatibility::HardConflict;
                    }
                    (RepoAccessMode::ReadOnly, RepoAccessMode::ReadWrite)
                    | (RepoAccessMode::ReadWrite, RepoAccessMode::ReadOnly) => {
                        return ScopeCompatibility::SoftConflict;
                    }
                    _ => {}
                }
            }
        }
    }
    ScopeCompatibility::Compatible
}

fn check_git_compatibility(a: &GitScope, b: &GitScope) -> ScopeCompatibility {
    // If both can commit, that's a hard conflict (could create merge conflicts)
    if a.can_commit() && b.can_commit() {
        return ScopeCompatibility::HardConflict;
    }
    ScopeCompatibility::Compatible
}

fn check_execution_compatibility(a: &ExecutionScope, b: &ExecutionScope) -> ScopeCompatibility {
    // Phase 1: conservative heuristics.
    // If either side allows everything, treat as soft conflict.
    // If both allow everything, treat as hard conflict.
    let a_allows_all = a.allowed.iter().any(|p| p == "*") && a.denied.is_empty();
    let b_allows_all = b.allowed.iter().any(|p| p == "*") && b.denied.is_empty();

    if a_allows_all && b_allows_all {
        return ScopeCompatibility::HardConflict;
    }
    if a_allows_all || b_allows_all {
        return ScopeCompatibility::SoftConflict;
    }

    ScopeCompatibility::Compatible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_rule_matching() {
        let rule = PathRule::write("src/");
        assert!(rule.matches("src/main.rs"));
        assert!(rule.matches("src/lib.rs"));
        assert!(!rule.matches("tests/test.rs"));
    }

    #[test]
    fn glob_matching() {
        let rule = PathRule::write("src/**/*.rs");
        assert!(rule.matches("src/main.rs"));
        assert!(rule.matches("src/core/mod.rs"));
    }

    #[test]
    fn filesystem_scope_permissions() {
        let scope = FilesystemScope::new()
            .with_rule(PathRule::write("src/"))
            .with_rule(PathRule::read("docs/"))
            .with_rule(PathRule::deny("src/secret/"));

        assert!(scope.can_write("src/main.rs"));
        assert!(scope.can_read("docs/README.md"));
        assert!(!scope.can_write("docs/README.md"));
        assert!(!scope.can_read("src/secret/key.txt"));
    }

    #[test]
    fn repository_scope_creation() {
        let ro = RepositoryScope::read_only("repo1");
        let rw = RepositoryScope::read_write("repo2");

        assert_eq!(ro.mode, RepoAccessMode::ReadOnly);
        assert_eq!(rw.mode, RepoAccessMode::ReadWrite);
    }

    #[test]
    fn git_scope_permissions() {
        let scope = GitScope::with_branch();
        assert!(scope.can_commit());
        assert!(scope.can_branch());

        let empty = GitScope::new();
        assert!(!empty.can_commit());
    }

    #[test]
    fn execution_scope_allowed() {
        let scope = ExecutionScope::new().allow("cargo").allow("npm").deny("rm");

        assert!(scope.is_allowed("cargo build"));
        assert!(scope.is_allowed("npm install"));
        assert!(!scope.is_allowed("rm -rf /"));
    }

    #[test]
    fn compatible_scopes() {
        let a = Scope::new()
            .with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/a/")));
        let b = Scope::new()
            .with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/b/")));

        assert_eq!(check_compatibility(&a, &b), ScopeCompatibility::Compatible);
    }

    #[test]
    fn hard_conflict_overlapping_writes() {
        let a =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));
        let b =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));

        assert_eq!(
            check_compatibility(&a, &b),
            ScopeCompatibility::HardConflict
        );
    }

    #[test]
    fn deny_rules_produce_hard_conflict_on_overlap() {
        let a =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::deny("src/")));
        let b =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::read("src/")));

        assert_eq!(
            check_compatibility(&a, &b),
            ScopeCompatibility::HardConflict
        );
    }

    #[test]
    fn soft_conflict_read_write() {
        let a =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::read("src/")));
        let b =
            Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));

        assert_eq!(
            check_compatibility(&a, &b),
            ScopeCompatibility::SoftConflict
        );
    }

    #[test]
    fn repository_conflict() {
        let a = Scope::new().with_repository(RepositoryScope::read_write("repo"));
        let b = Scope::new().with_repository(RepositoryScope::read_write("repo"));

        assert_eq!(
            check_compatibility(&a, &b),
            ScopeCompatibility::HardConflict
        );
    }

    #[test]
    fn git_commit_conflict() {
        let a = Scope::new().with_git(GitScope::with_commit());
        let b = Scope::new().with_git(GitScope::with_commit());

        assert_eq!(
            check_compatibility(&a, &b),
            ScopeCompatibility::HardConflict
        );
    }

    #[test]
    fn scope_serialization() {
        let scope = Scope::new()
            .with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")))
            .with_repository(RepositoryScope::read_write("my-repo"))
            .with_git(GitScope::with_commit());

        let json = serde_json::to_string(&scope).unwrap();
        let restored: Scope = serde_json::from_str(&json).unwrap();

        assert_eq!(scope, restored);
    }
}
