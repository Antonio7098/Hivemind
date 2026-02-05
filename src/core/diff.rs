//! Baseline and diff computation for change detection.
//!
//! Captures filesystem state before execution and computes changes
//! after execution for verification and attribution.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// File hash (SHA-256 hex string).
pub type FileHash = String;

/// A snapshot of a file at a point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Relative path from root.
    pub path: PathBuf,
    /// File hash (None if directory or unreadable).
    pub hash: Option<FileHash>,
    /// File size in bytes.
    pub size: u64,
    /// Whether this is a directory.
    pub is_dir: bool,
}

/// A baseline snapshot of a directory tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    /// Unique baseline ID.
    pub id: Uuid,
    /// Root path this baseline was taken from.
    pub root: PathBuf,
    /// Git HEAD commit at baseline time.
    pub git_head: Option<String>,
    /// File snapshots by relative path.
    pub files: HashMap<PathBuf, FileSnapshot>,
    /// Timestamp when baseline was captured.
    pub captured_at: chrono::DateTime<chrono::Utc>,
}

impl Baseline {
    /// Captures a baseline from a directory.
    pub fn capture(root: &Path) -> io::Result<Self> {
        let mut files = HashMap::new();
        capture_recursive(root, root, &mut files)?;

        let git_head = get_git_head(root).ok();

        Ok(Self {
            id: Uuid::new_v4(),
            root: root.to_path_buf(),
            git_head,
            files,
            captured_at: chrono::Utc::now(),
        })
    }

    /// Gets a file snapshot by path.
    pub fn get(&self, path: &Path) -> Option<&FileSnapshot> {
        self.files.get(path)
    }

    /// Returns the number of files in the baseline.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

fn capture_recursive(
    root: &Path,
    current: &Path,
    files: &mut HashMap<PathBuf, FileSnapshot>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

        // Skip .git directory
        if relative.starts_with(".git") {
            continue;
        }

        let metadata = entry.metadata()?;
        let is_dir = metadata.is_dir();

        let hash = if is_dir {
            None
        } else {
            compute_hash(&path).ok()
        };

        files.insert(
            relative.clone(),
            FileSnapshot {
                path: relative,
                hash,
                size: metadata.len(),
                is_dir,
            },
        );

        if is_dir {
            capture_recursive(root, &path, files)?;
        }
    }

    Ok(())
}

fn compute_hash(path: &Path) -> io::Result<FileHash> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut file = fs::File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let mut hasher = DefaultHasher::new();
    contents.hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn get_git_head(path: &Path) -> io::Result<String> {
    use std::process::Command;

    let output = Command::new("git")
        .current_dir(path)
        .args(["rev-parse", "HEAD"])
        .output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to get git HEAD",
        ))
    }
}

/// Type of change detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

/// A detected file change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// Relative path.
    pub path: PathBuf,
    /// Type of change.
    pub change_type: ChangeType,
    /// Old hash (for modified/deleted).
    pub old_hash: Option<FileHash>,
    /// New hash (for created/modified).
    pub new_hash: Option<FileHash>,
}

/// A diff between two states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff {
    /// Unique diff ID.
    pub id: Uuid,
    /// Task this diff is attributed to.
    pub task_id: Option<Uuid>,
    /// Attempt this diff is attributed to.
    pub attempt_id: Option<Uuid>,
    /// Base state (before).
    pub baseline_id: Uuid,
    /// File changes.
    pub changes: Vec<FileChange>,
    /// Computed timestamp.
    pub computed_at: chrono::DateTime<chrono::Utc>,
}

impl Diff {
    /// Computes a diff between a baseline and current state.
    pub fn compute(baseline: &Baseline, current_root: &Path) -> io::Result<Self> {
        let mut current_files = HashMap::new();
        capture_recursive(current_root, current_root, &mut current_files)?;

        let mut changes = Vec::new();

        // Check for modified and deleted files
        for (path, old_snapshot) in &baseline.files {
            match current_files.get(path) {
                Some(new_snapshot) => {
                    if old_snapshot.hash != new_snapshot.hash {
                        changes.push(FileChange {
                            path: path.clone(),
                            change_type: ChangeType::Modified,
                            old_hash: old_snapshot.hash.clone(),
                            new_hash: new_snapshot.hash.clone(),
                        });
                    }
                }
                None => {
                    changes.push(FileChange {
                        path: path.clone(),
                        change_type: ChangeType::Deleted,
                        old_hash: old_snapshot.hash.clone(),
                        new_hash: None,
                    });
                }
            }
        }

        // Check for created files
        for (path, new_snapshot) in &current_files {
            if !baseline.files.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    change_type: ChangeType::Created,
                    old_hash: None,
                    new_hash: new_snapshot.hash.clone(),
                });
            }
        }

        // Sort changes by path for deterministic output
        changes.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self {
            id: Uuid::new_v4(),
            task_id: None,
            attempt_id: None,
            baseline_id: baseline.id,
            changes,
            computed_at: chrono::Utc::now(),
        })
    }

    /// Attributes this diff to a task.
    pub fn for_task(mut self, task_id: Uuid) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Attributes this diff to an attempt.
    pub fn for_attempt(mut self, attempt_id: Uuid) -> Self {
        self.attempt_id = Some(attempt_id);
        self
    }

    /// Returns true if there are no changes.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the number of changes.
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// Returns changes of a specific type.
    pub fn changes_of_type(&self, change_type: ChangeType) -> Vec<&FileChange> {
        self.changes
            .iter()
            .filter(|c| c.change_type == change_type)
            .collect()
    }

    /// Returns all modified paths.
    pub fn modified_paths(&self) -> Vec<&Path> {
        self.changes.iter().map(|c| c.path.as_path()).collect()
    }
}

/// Unified diff format for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedDiff {
    /// File path.
    pub path: PathBuf,
    /// Diff content in unified format.
    pub content: String,
}

/// Computes unified diff between two files.
pub fn unified_diff(old_path: Option<&Path>, new_path: Option<&Path>) -> io::Result<String> {
    use std::process::Command;

    let (old, new) = match (old_path, new_path) {
        (Some(old), Some(new)) => (old.to_str().unwrap_or(""), new.to_str().unwrap_or("")),
        (Some(old), None) => (old.to_str().unwrap_or(""), "/dev/null"),
        (None, Some(new)) => ("/dev/null", new.to_str().unwrap_or("")),
        (None, None) => return Ok(String::new()),
    };

    let output = Command::new("diff").args(["-u", old, new]).output()?;

    // diff returns 1 if files differ, which is expected
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
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

        // Create a new file
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

        // Modify a file
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

        // Delete a file
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
}
