use super::*;

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
    for rule_a in &a.rules {
        for rule_b in &b.rules {
            if patterns_might_overlap(&rule_a.pattern, &rule_b.pattern) {
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
    if a.contains('*') || b.contains('*') {
        return true;
    }
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
    if a.can_commit() && b.can_commit() {
        return ScopeCompatibility::HardConflict;
    }
    ScopeCompatibility::Compatible
}

fn check_execution_compatibility(a: &ExecutionScope, b: &ExecutionScope) -> ScopeCompatibility {
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
