//! Worktree management for isolated task execution.
//!
//! Worktrees provide isolated git working directories for parallel
//! task execution without branch conflicts.

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
        // Verify it's a git repository
        let git_dir = repo_path.join(".git");
        if !git_dir.exists() {
            return Err(WorktreeError::InvalidRepo(repo_path));
        }

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
        let branch_name = format!("hivemind/{}/{}", flow_id, task_id);
        let worktree_path = self
            .config
            .base_dir
            .join(flow_id.to_string())
            .join(task_id.to_string());

        // Create parent directories
        std::fs::create_dir_all(&worktree_path)?;

        // Get base commit
        let base = base_ref.unwrap_or("HEAD");
        let base_commit = self.get_commit_hash(base)?;

        // Create worktree with new branch
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                worktree_path.to_str().unwrap_or(""),
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
        path.join(".git").exists()
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

    // Note: Full worktree tests require a real git repository
    // and are better suited for integration tests
}
