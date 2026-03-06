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

mod baseline;
mod compute;

#[cfg(test)]
mod tests;

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
    #[serde(default)]
    pub git_branches: Vec<String>,
    /// File snapshots by relative path.
    pub files: HashMap<PathBuf, FileSnapshot>,
    /// Timestamp when baseline was captured.
    pub captured_at: chrono::DateTime<chrono::Utc>,
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

/// Unified diff format for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedDiff {
    /// File path.
    pub path: PathBuf,
    /// Diff content in unified format.
    pub content: String,
}

pub use compute::unified_diff;
