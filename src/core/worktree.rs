//! Worktree management for isolated task execution.
//!
//! Worktrees provide isolated git working directories for parallel
//! task execution without branch conflicts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

mod git;
mod inspect;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct WorktreeConfig {
    pub base_dir: PathBuf,
    pub cleanup_on_success: bool,
    pub preserve_on_failure: bool,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        let base_dir = std::env::var("HIVEMIND_WORKTREE_DIR").map_or_else(
            |_| {
                dirs::home_dir().map_or_else(
                    || PathBuf::from("hivemind/worktrees"),
                    |home| home.join("hivemind").join("worktrees"),
                )
            },
            PathBuf::from,
        );
        Self {
            base_dir,
            cleanup_on_success: true,
            preserve_on_failure: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub id: Uuid,
    pub task_id: Uuid,
    pub flow_id: Uuid,
    pub workflow_id: Option<Uuid>,
    pub workflow_run_id: Option<Uuid>,
    pub step_id: Option<Uuid>,
    pub step_run_id: Option<Uuid>,
    pub path: PathBuf,
    pub branch: String,
    pub base_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeStatus {
    pub flow_id: Uuid,
    pub task_id: Uuid,
    #[serde(default)]
    pub workflow_id: Option<Uuid>,
    #[serde(default)]
    pub workflow_run_id: Option<Uuid>,
    #[serde(default)]
    pub step_id: Option<Uuid>,
    #[serde(default)]
    pub step_run_id: Option<Uuid>,
    pub path: PathBuf,
    pub is_worktree: bool,
    pub head_commit: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug)]
pub enum WorktreeError {
    GitError(String),
    IoError(std::io::Error),
    NotFound(Uuid),
    AlreadyExists(Uuid),
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

pub type Result<T> = std::result::Result<T, WorktreeError>;

pub struct WorktreeManager {
    repo_path: PathBuf,
    config: WorktreeConfig,
}
