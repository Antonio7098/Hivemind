//! Worktree management for isolated task execution.
//!
//! Worktrees provide isolated git working directories for parallel
//! task execution without branch conflicts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

/// Worktree configuration.
#[derive(Debug, Clone)]
pub struct WorktreeConfig {
    /// Base directory for worktrees.
    pub base_dir: PathBuf,
    /// Whether to clean up worktrees on success.
    pub cleanup_on_success: bool,
    /// Whether to preserve worktrees on failure.
    pub preserve_on_failure: bool,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            base_dir: PathBuf::from(".hivemind/worktrees"),
            cleanup_on_success: true,
            preserve_on_failure: true,
        }
    }
}

/// Information about a worktree.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    /// Worktree ID.
    pub id: Uuid,
    /// Task this worktree is for.
    pub task_id: Uuid,
    /// Flow this worktree belongs to.
    pub flow_id: Uuid,
    /// Path to the worktree.
    pub path: PathBuf,
    /// Branch name in the worktree.
    pub branch: String,
    /// Base commit the worktree was created from.
    pub base_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStatus {
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub path: PathBuf,
    pub is_worktree: bool,
    pub head_commit: Option<String>,
    pub branch: Option<String>,
}

/// Errors that can occur during worktree operations.
#[derive(Debug)]
pub enum WorktreeError {
    /// Git command failed.
    GitError(String),
    /// IO error.
    IoError(std::io::Error),
    /// Worktree not found.
    NotFound(Uuid),
    /// Worktree already exists.
    AlreadyExists(Uuid),
    /// Invalid repository path.
    InvalidRepo(PathBuf),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitError(msg) => write!(f, "Git error: {msg}"),
            Self::IoError(e) => write!(f, "IO error: {e}"),
            Self::NotFound(id) => write!(f, "Worktree not found: {id}"),
            Self::AlreadyExists(id) => write!(f, "Worktree already exists: {id}"),
            Self::InvalidRepo(path) => write!(f, "Invalid repository: {}", path.display()),
        }
    }
}

impl std::error::Error for WorktreeError {}

impl From<std::io::Error> for WorktreeError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

/// Result type for worktree operations.
pub type Result<T> = std::result::Result<T, WorktreeError>;

/// Manager for git worktrees.
pub struct WorktreeManager {
    /// Repository root path.
    repo_path: PathBuf,
    /// Configuration.
    config: WorktreeConfig,
}

impl WorktreeManager {
    /// Creates a new worktree manager.
    pub fn new(repo_path: PathBuf, config: WorktreeConfig) -> Result<Self> {
        let WorktreeConfig {
            base_dir,
            cleanup_on_success,
            preserve_on_failure,
        } = config;

        // Verify it's a git repository
        let is_git_repo = Command::new("git")
            .current_dir(&repo_path)
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !is_git_repo {
            return Err(WorktreeError::InvalidRepo(repo_path));
        }

        let base_dir = if base_dir.is_absolute() {
            base_dir
        } else {
            repo_path.join(base_dir)
        };

        let config = WorktreeConfig {
            base_dir,
            cleanup_on_success,
            preserve_on_failure,
        };

        Ok(Self { repo_path, config })
    }

    /// Creates a worktree for a task.
    pub fn create(
        &self,
        flow_id: Uuid,
        task_id: Uuid,
        base_ref: Option<&str>,
    ) -> Result<WorktreeInfo> {
        let worktree_id = Uuid::new_v4();
        let branch_name = format!("exec/{flow_id}/{task_id}");
        let worktree_path = self
            .config
            .base_dir
            .join(flow_id.to_string())
            .join(task_id.to_string());

        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(task_id));
        }

        // Create parent directories
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Get base commit
        let base = base_ref.unwrap_or("HEAD");
        let base_commit = self.get_commit_hash(base)?;

        // Create worktree with new branch
        let worktree_path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::GitError("Worktree path is not valid UTF-8".to_string())
        })?;
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args([
                "worktree",
                "add",
                "-B",
                &branch_name,
                worktree_path_str,
                base,
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(WorktreeInfo {
            id: worktree_id,
            task_id,
            flow_id,
            path: worktree_path,
            branch: branch_name,
            base_commit,
        })
    }

    /// Removes a worktree.
    pub fn remove(&self, worktree_path: &Path) -> Result<()> {
        // Remove worktree
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap_or(""),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(())
    }

    /// Returns the default worktree path for a flow/task pair.
    #[must_use]
    pub fn path_for(&self, flow_id: Uuid, task_id: Uuid) -> PathBuf {
        self.config
            .base_dir
            .join(flow_id.to_string())
            .join(task_id.to_string())
    }

    /// Inspects an existing worktree for a flow/task.
    pub fn inspect(&self, flow_id: Uuid, task_id: Uuid) -> Result<WorktreeStatus> {
        let path = self.path_for(flow_id, task_id);
        if !path.exists() {
            return Ok(WorktreeStatus {
                flow_id,
                task_id,
                path,
                is_worktree: false,
                head_commit: None,
                branch: None,
            });
        }

        let is_worktree = self.is_worktree(&path);
        let head_commit = if is_worktree {
            self.worktree_head(&path).ok()
        } else {
            None
        };

        let branch = if is_worktree {
            let output = Command::new("git")
                .current_dir(&path)
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                }
                _ => None,
            }
        } else {
            None
        };

        Ok(WorktreeStatus {
            flow_id,
            task_id,
            path,
            is_worktree,
            head_commit,
            branch,
        })
    }

    /// Lists all worktrees for a flow.
    pub fn list_for_flow(&self, flow_id: Uuid) -> Result<Vec<PathBuf>> {
        let flow_dir = self.config.base_dir.join(flow_id.to_string());

        if !flow_dir.exists() {
            return Ok(Vec::new());
        }

        let mut worktrees = Vec::new();
        for entry in std::fs::read_dir(&flow_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                worktrees.push(entry.path());
            }
        }

        Ok(worktrees)
    }

    /// Cleans up all worktrees for a flow.
    pub fn cleanup_flow(&self, flow_id: Uuid) -> Result<()> {
        let worktrees = self.list_for_flow(flow_id)?;

        for path in worktrees {
            self.remove(&path)?;
        }

        // Remove flow directory
        let flow_dir = self.config.base_dir.join(flow_id.to_string());
        if flow_dir.exists() {
            std::fs::remove_dir_all(&flow_dir)?;
        }

        Ok(())
    }

    /// Gets the current commit hash for a ref.
    fn get_commit_hash(&self, reference: &str) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["rev-parse", reference])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Checks if a path is a valid worktree.
    pub fn is_worktree(&self, path: &Path) -> bool {
        if !path.join(".git").exists() {
            return false;
        }

        let output = Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();

        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .trim()
                .eq_ignore_ascii_case("true"),
            _ => false,
        }
    }

    /// Gets the HEAD commit of a worktree.
    pub fn worktree_head(&self, worktree_path: &Path) -> Result<String> {
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Creates a commit in a worktree.
    pub fn commit(&self, worktree_path: &Path, message: &str) -> Result<String> {
        // Stage all changes
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["add", "-A"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        // Commit
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["commit", "-m", message, "--allow-empty"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(WorktreeError::GitError(stderr.to_string()));
        }

        self.worktree_head(worktree_path)
    }

    /// Returns the repository path.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Returns the config.
    pub fn config(&self) -> &WorktreeConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_git_repo(repo_dir: &Path) {
        std::fs::create_dir_all(repo_dir).expect("create repo dir");

        let out = Command::new("git")
            .args(["init"])
            .current_dir(repo_dir)
            .output()
            .expect("git init");
        assert!(out.status.success(), "git init failed");

        let out = Command::new("git")
            .args(["config", "user.name", "Hivemind"])
            .current_dir(repo_dir)
            .output()
            .expect("git config user.name");
        assert!(out.status.success(), "git config user.name failed");

        let out = Command::new("git")
            .args(["config", "user.email", "hivemind@example.com"])
            .current_dir(repo_dir)
            .output()
            .expect("git config user.email");
        assert!(out.status.success(), "git config user.email failed");

        std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");

        let out = Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir)
            .output()
            .expect("git add");
        assert!(out.status.success(), "git add failed");

        let out = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo_dir)
            .output()
            .expect("git commit");
        assert!(out.status.success(), "git commit failed");
    }

    #[test]
    fn worktree_config_default() {
        let config = WorktreeConfig::default();
        assert!(config.cleanup_on_success);
        assert!(config.preserve_on_failure);
    }

    #[test]
    fn worktree_info_creation() {
        let info = WorktreeInfo {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            flow_id: Uuid::new_v4(),
            path: PathBuf::from("/tmp/test"),
            branch: "test-branch".to_string(),
            base_commit: "abc123".to_string(),
        };

        assert!(!info.branch.is_empty());
    }

    #[test]
    fn invalid_repo_detection() {
        let result = WorktreeManager::new(
            PathBuf::from("/nonexistent/path"),
            WorktreeConfig::default(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn create_inspect_list_commit_and_cleanup() {
        let tmp = tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let manager =
            WorktreeManager::new(repo_dir, WorktreeConfig::default()).expect("worktree manager");

        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let info = manager
            .create(flow_id, task_id, None)
            .expect("create worktree");
        assert!(info.path.exists());
        assert!(manager.is_worktree(&info.path));

        let status = manager.inspect(flow_id, task_id).expect("inspect");
        assert!(status.is_worktree);
        assert_eq!(status.flow_id, flow_id);
        assert_eq!(status.task_id, task_id);
        assert!(status.head_commit.is_some());

        let listed = manager.list_for_flow(flow_id).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], info.path);

        std::fs::write(info.path.join("file.txt"), "hello\n").expect("write file");
        let head = manager.commit(&info.path, "commit").expect("commit");
        assert!(!head.trim().is_empty());

        manager.cleanup_flow(flow_id).expect("cleanup");
        assert!(!info.path.exists());
    }

    // Note: Full worktree tests require a real git repository
    // and are better suited for integration tests
}
