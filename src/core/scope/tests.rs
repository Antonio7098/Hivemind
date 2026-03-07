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
    let a =
        Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/a/")));
    let b =
        Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/b/")));

    assert_eq!(check_compatibility(&a, &b), ScopeCompatibility::Compatible);
}

#[test]
fn hard_conflict_overlapping_writes() {
    let a = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));
    let b = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));

    assert_eq!(
        check_compatibility(&a, &b),
        ScopeCompatibility::HardConflict
    );
}

#[test]
fn deny_rules_produce_hard_conflict_on_overlap() {
    let a = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::deny("src/")));
    let b = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::read("src/")));

    assert_eq!(
        check_compatibility(&a, &b),
        ScopeCompatibility::HardConflict
    );
}

#[test]
fn soft_conflict_read_write() {
    let a = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::read("src/")));
    let b = Scope::new().with_filesystem(FilesystemScope::new().with_rule(PathRule::write("src/")));

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
