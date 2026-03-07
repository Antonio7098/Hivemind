use super::*;
use crate::core::diff::{ChangeType, Diff, FileChange};
use crate::core::scope::{FilesystemScope, GitScope, PathRule, Scope};
use std::path::PathBuf;

fn test_scope() -> Scope {
    Scope::new()
        .with_filesystem(
            FilesystemScope::new()
                .with_rule(PathRule::write("src/"))
                .with_rule(PathRule::read("docs/"))
                .with_rule(PathRule::deny("secrets/")),
        )
        .with_git(GitScope::with_commit())
}

fn test_diff_with_changes(changes: Vec<FileChange>) -> Diff {
    Diff {
        id: Uuid::new_v4(),
        task_id: None,
        attempt_id: None,
        baseline_id: Uuid::new_v4(),
        changes,
        computed_at: chrono::Utc::now(),
    }
}

#[test]
fn allowed_write_passes() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = test_diff_with_changes(vec![FileChange {
        path: PathBuf::from("src/main.rs"),
        change_type: ChangeType::Modified,
        old_hash: Some("old".to_string()),
        new_hash: Some("new".to_string()),
    }]);

    let result = enforcer.verify_diff(&diff, task_id, attempt_id);
    assert!(result.passed);
    assert!(result.violations.is_empty());
}

#[test]
fn disallowed_write_fails() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = test_diff_with_changes(vec![FileChange {
        path: PathBuf::from("docs/README.md"),
        change_type: ChangeType::Modified,
        old_hash: Some("old".to_string()),
        new_hash: Some("new".to_string()),
    }]);

    let result = enforcer.verify_diff(&diff, task_id, attempt_id);
    assert!(!result.passed);
    assert_eq!(result.violations.len(), 1);
    assert_eq!(
        result.violations[0].violation_type,
        ViolationType::Filesystem
    );
}

#[test]
fn denied_path_fails() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = test_diff_with_changes(vec![FileChange {
        path: PathBuf::from("secrets/api_key.txt"),
        change_type: ChangeType::Created,
        old_hash: None,
        new_hash: Some("new".to_string()),
    }]);

    let result = enforcer.verify_diff(&diff, task_id, attempt_id);
    assert!(!result.passed);
}

#[test]
fn git_commit_allowed() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let result = enforcer.verify_git_operations(true, false, task_id, attempt_id);
    assert!(result.passed);
}

#[test]
fn git_commit_disallowed() {
    let scope = Scope::new().with_git(GitScope::new());
    let enforcer = ScopeEnforcer::new(scope);
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let result = enforcer.verify_git_operations(true, false, task_id, attempt_id);
    assert!(!result.passed);
    assert_eq!(result.violations[0].violation_type, ViolationType::Git);
}

#[test]
fn git_branch_disallowed() {
    let scope = Scope::new().with_git(GitScope::with_commit());
    let enforcer = ScopeEnforcer::new(scope);
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let result = enforcer.verify_git_operations(false, true, task_id, attempt_id);
    assert!(!result.passed);
}

#[test]
fn full_verification() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = test_diff_with_changes(vec![FileChange {
        path: PathBuf::from("src/lib.rs"),
        change_type: ChangeType::Created,
        old_hash: None,
        new_hash: Some("new".to_string()),
    }]);

    let result = enforcer.verify_all(&diff, true, false, task_id, attempt_id);
    assert!(result.passed);
}

#[test]
fn full_verification_with_violations() {
    let enforcer = ScopeEnforcer::new(test_scope());
    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = test_diff_with_changes(vec![
        FileChange {
            path: PathBuf::from("src/lib.rs"),
            change_type: ChangeType::Created,
            old_hash: None,
            new_hash: Some("new".to_string()),
        },
        FileChange {
            path: PathBuf::from("config/settings.json"),
            change_type: ChangeType::Modified,
            old_hash: Some("old".to_string()),
            new_hash: Some("new".to_string()),
        },
    ]);

    let result = enforcer.verify_all(&diff, true, true, task_id, attempt_id);
    assert!(!result.passed);
    assert!(result.violations.len() >= 2);
}

#[test]
fn violation_serialization() {
    let violation = ScopeViolation::filesystem("test.txt", "Write not allowed");
    let json = serde_json::to_string(&violation).unwrap();
    let restored: ScopeViolation = serde_json::from_str(&json).unwrap();

    assert_eq!(violation, restored);
}

#[test]
fn verification_result_serialization() {
    let result = VerificationResult::pass(Uuid::new_v4(), Uuid::new_v4());
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"passed\":true"));
}

#[test]
fn path_matching() {
    let patterns = vec!["src/".to_string(), "tests/*".to_string()];

    assert!(path_matches_any(Path::new("src/main.rs"), &patterns));
    assert!(path_matches_any(Path::new("tests/test1.rs"), &patterns));
    assert!(!path_matches_any(Path::new("docs/README.md"), &patterns));
}
