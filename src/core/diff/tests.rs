use super::*;
use tempfile::TempDir;

fn create_test_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file1.txt"), "content1").unwrap();
    fs::write(dir.path().join("file2.txt"), "content2").unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();
    fs::write(dir.path().join("subdir/file3.txt"), "content3").unwrap();
    dir
}

#[test]
fn capture_baseline() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    assert!(baseline.file_count() >= 3);
    assert!(baseline.get(Path::new("file1.txt")).is_some());
}

#[test]
fn detect_created_file() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    fs::write(dir.path().join("new_file.txt"), "new content").unwrap();

    let diff = Diff::compute(&baseline, dir.path()).unwrap();

    let created: Vec<_> = diff.changes_of_type(ChangeType::Created);
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].path, Path::new("new_file.txt"));
}

#[test]
fn detect_modified_file() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    fs::write(dir.path().join("file1.txt"), "modified content").unwrap();

    let diff = Diff::compute(&baseline, dir.path()).unwrap();

    let modified: Vec<_> = diff.changes_of_type(ChangeType::Modified);
    assert_eq!(modified.len(), 1);
    assert_eq!(modified[0].path, Path::new("file1.txt"));
}

#[test]
fn detect_deleted_file() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    fs::remove_file(dir.path().join("file1.txt")).unwrap();

    let diff = Diff::compute(&baseline, dir.path()).unwrap();

    let deleted: Vec<_> = diff.changes_of_type(ChangeType::Deleted);
    assert_eq!(deleted.len(), 1);
    assert_eq!(deleted[0].path, Path::new("file1.txt"));
}

#[test]
fn no_changes_empty_diff() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    let diff = Diff::compute(&baseline, dir.path()).unwrap();

    assert!(diff.is_empty());
}

#[test]
fn diff_attribution() {
    let dir = create_test_dir();
    let baseline = Baseline::capture(dir.path()).unwrap();

    let task_id = Uuid::new_v4();
    let attempt_id = Uuid::new_v4();

    let diff = Diff::compute(&baseline, dir.path())
        .unwrap()
        .for_task(task_id)
        .for_attempt(attempt_id);

    assert_eq!(diff.task_id, Some(task_id));
    assert_eq!(diff.attempt_id, Some(attempt_id));
}

#[test]
fn file_change_serialization() {
    let change = FileChange {
        path: PathBuf::from("test.txt"),
        change_type: ChangeType::Modified,
        old_hash: Some("abc".to_string()),
        new_hash: Some("def".to_string()),
    };

    let json = serde_json::to_string(&change).unwrap();
    let restored: FileChange = serde_json::from_str(&json).unwrap();

    assert_eq!(change, restored);
}
