//! `OpenCode` adapter - wrapper for `OpenCode` CLI runtime.
//!
//! This adapter wraps the `OpenCode` CLI to provide task execution
//! capabilities within Hivemind's orchestration framework.

use super::runtime::{
    deterministic_env_pairs, format_execution_prompt, AdapterConfig, ExecutionInput,
    ExecutionReport, InteractiveAdapterEvent, InteractiveExecutionResult, RuntimeAdapter,
    RuntimeError,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

mod config;
mod interactive;
pub use config::*;
mod runtime_impl;

/// `OpenCode` runtime adapter.
pub struct OpenCodeAdapter {
    config: OpenCodeConfig,
    worktree: Option<PathBuf>,
    task_id: Option<Uuid>,
    process: Option<Child>,
}
impl OpenCodeAdapter {
    /// Creates a new `OpenCode` adapter.
    pub fn new(config: OpenCodeConfig) -> Self {
        Self {
            config,
            worktree: None,
            task_id: None,
            process: None,
        }
    }

    /// Creates an adapter with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OpenCodeConfig::default())
    }

    /// Formats the input for the runtime.
    #[allow(clippy::unused_self)]
    fn format_input(&self, input: &ExecutionInput) -> String {
        format_execution_prompt(input)
    }

    fn capture_worktree_state(root: &Path) -> BTreeMap<PathBuf, FileFingerprint> {
        let mut state = BTreeMap::new();
        Self::capture_worktree_state_recursive(root, root, &mut state);
        state
    }

    fn capture_worktree_state_recursive(
        root: &Path,
        current: &Path,
        state: &mut BTreeMap<PathBuf, FileFingerprint>,
    ) {
        let Ok(entries) = fs::read_dir(current) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };

            if relative
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == ".git")
            {
                continue;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                Self::capture_worktree_state_recursive(root, &path, state);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let modified_unix_secs = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_secs());
            state.insert(
                relative.to_path_buf(),
                FileFingerprint {
                    len: metadata.len(),
                    modified_unix_secs,
                },
            );
        }
    }

    fn diff_worktree_state(
        before: &BTreeMap<PathBuf, FileFingerprint>,
        after: &BTreeMap<PathBuf, FileFingerprint>,
    ) -> FilesystemDelta {
        let mut created = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        for (path, fingerprint) in after {
            match before.get(path) {
                None => created.push(path.clone()),
                Some(previous) if previous != fingerprint => modified.push(path.clone()),
                Some(_) => {}
            }
        }

        for path in before.keys() {
            if !after.contains_key(path) {
                deleted.push(path.clone());
            }
        }

        FilesystemDelta {
            created,
            modified,
            deleted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    modified_unix_secs: u64,
}

#[derive(Debug, Default)]
struct FilesystemDelta {
    created: Vec<PathBuf>,
    modified: Vec<PathBuf>,
    deleted: Vec<PathBuf>,
}

#[cfg(test)]
mod tests;
