//! Project registry for managing projects via events.
//!
//! The registry derives project state from events and provides
//! operations that emit new events.

use crate::core::diff::{unified_diff, Baseline, ChangeType, Diff, FileChange};
use crate::core::enforcement::{ScopeEnforcer, VerificationResult};
use crate::core::error::{ErrorCategory, HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload, RuntimeOutputStream, RuntimeRole};
use crate::core::flow::{FlowState, RetryMode, RunMode, TaskExecState, TaskFlow};
use crate::core::graph::{GraphState, GraphTask, RetryPolicy, SuccessCriteria, TaskGraph};
use crate::core::runtime_event_projection::{ProjectedRuntimeObservation, RuntimeEventProjector};
use crate::core::scope::{check_compatibility, RepoAccessMode, Scope, ScopeCompatibility};
use crate::core::state::{
    AppState, AttemptCheckpoint, AttemptCheckpointState, AttemptState, Project,
    ProjectRuntimeConfig, RuntimeRoleDefaults, Task, TaskRuntimeConfig, TaskState,
};
use crate::core::worktree::{WorktreeConfig, WorktreeError, WorktreeManager, WorktreeStatus};
use crate::storage::event_store::{EventFilter, EventStore, IndexedEventStore};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::adapters::claude_code::{ClaudeCodeAdapter, ClaudeCodeConfig};
use crate::adapters::codex::{CodexAdapter, CodexConfig};
use crate::adapters::kilo::{KiloAdapter, KiloConfig};
use crate::adapters::opencode::OpenCodeConfig;
use crate::adapters::runtime::{
    format_execution_prompt, AttemptSummary, ExecutionInput, InteractiveAdapterEvent,
    InteractiveExecutionResult, RuntimeAdapter, RuntimeError,
};
use crate::adapters::{runtime_descriptors, SUPPORTED_ADAPTERS};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeExecuteMode {
    #[default]
    Local,
    Pr,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct MergeExecuteOptions {
    #[serde(default)]
    pub mode: MergeExecuteMode,
    #[serde(default)]
    pub monitor_ci: bool,
    #[serde(default)]
    pub auto_merge: bool,
    #[serde(default)]
    pub pull_after: bool,
}

/// Configuration for the registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// Base directory for hivemind data.
    pub data_dir: PathBuf,
}

impl RegistryConfig {
    /// Creates a new config with default data directory.
    #[must_use]
    pub fn default_dir() -> Self {
        if let Ok(data_dir) = env::var("HIVEMIND_DATA_DIR") {
            return Self {
                data_dir: PathBuf::from(data_dir),
            };
        }

        let data_dir =
            dirs::home_dir().map_or_else(|| PathBuf::from(".hivemind"), |h| h.join(".hivemind"));
        Self { data_dir }
    }

    /// Creates a config with custom data directory.
    #[must_use]
    pub fn with_dir(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Returns the path to the global events file.
    #[must_use]
    pub fn events_path(&self) -> PathBuf {
        self.data_dir.join("events.jsonl")
    }
}

/// The project registry manages projects via event sourcing.
pub struct Registry {
    store: Arc<dyn EventStore>,
    config: RegistryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeListEntry {
    pub adapter_name: String,
    pub default_binary: String,
    pub available: bool,
    pub opencode_compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeHealthStatus {
    pub adapter_name: String,
    pub binary_path: String,
    pub healthy: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiffArtifact {
    diff: Diff,
    unified: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScopeRepoSnapshot {
    repo_path: String,
    #[serde(default)]
    git_head: Option<String>,
    #[serde(default)]
    status_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScopeBaselineArtifact {
    attempt_id: Uuid,
    #[serde(default)]
    repo_snapshots: Vec<ScopeRepoSnapshot>,
    #[serde(default)]
    tmp_entries: Vec<String>,
}

struct CompletionArtifacts<'a> {
    baseline_id: Uuid,
    artifact: &'a DiffArtifact,
    checkpoint_commit_sha: Option<String>,
}

struct CheckpointCommitSpec<'a> {
    flow_id: Uuid,
    task_id: Uuid,
    attempt_id: Uuid,
    checkpoint_id: &'a str,
    order: u32,
    total: u32,
    summary: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphValidationResult {
    pub graph_id: Uuid,
    pub valid: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointCompletionResult {
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_id: Uuid,
    pub checkpoint_id: String,
    pub order: u32,
    pub total: u32,
    #[serde(default)]
    pub next_checkpoint_id: Option<String>,
    pub all_completed: bool,
    pub commit_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttemptListItem {
    pub attempt_id: Uuid,
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_number: u32,
    pub started_at: chrono::DateTime<Utc>,
    pub all_checkpoints_completed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeCleanupResult {
    pub flow_id: Uuid,
    pub cleaned_worktrees: usize,
    pub forced: bool,
    pub dry_run: bool,
}

const GOVERNANCE_SCHEMA_VERSION: &str = "governance.v1";
const GOVERNANCE_PROJECTION_VERSION: u32 = 1;
const GOVERNANCE_FROM_LAYOUT: &str = "repo_local_hivemind_v1";
const GOVERNANCE_TO_LAYOUT: &str = "global_governance_v1";
const GOVERNANCE_EXPORT_IMPORT_BOUNDARY: &str = "Manual export/import only (not auto-enabled).";

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceArtifactInspect {
    pub scope: String,
    pub artifact_kind: String,
    pub artifact_key: String,
    pub path: String,
    pub exists: bool,
    pub projected: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GovernanceMigrationSummary {
    pub from_layout: String,
    pub to_layout: String,
    pub migrated_paths: Vec<String>,
    pub rollback_hint: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub migrated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceInitResult {
    pub project_id: Uuid,
    pub root_path: String,
    pub schema_version: String,
    pub projection_version: u32,
    pub created_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceMigrateResult {
    pub project_id: Uuid,
    pub from_layout: String,
    pub to_layout: String,
    pub migrated_paths: Vec<String>,
    pub rollback_hint: String,
    pub schema_version: String,
    pub projection_version: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectGovernanceInspectResult {
    pub project_id: Uuid,
    pub root_path: String,
    pub initialized: bool,
    pub schema_version: String,
    pub projection_version: u32,
    pub export_import_boundary: String,
    pub worktree_base_dir: String,
    pub artifacts: Vec<GovernanceArtifactInspect>,
    pub migrations: Vec<GovernanceMigrationSummary>,
    pub legacy_candidates: Vec<String>,
}

#[derive(Debug, Clone)]
struct GovernanceArtifactLocation {
    project_id: Option<Uuid>,
    scope: &'static str,
    artifact_kind: &'static str,
    artifact_key: String,
    is_dir: bool,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct LegacyGovernanceArtifactMapping {
    source: PathBuf,
    destination: GovernanceArtifactLocation,
}

enum SelectedRuntimeAdapter {
    OpenCode(crate::adapters::opencode::OpenCodeAdapter),
    Codex(CodexAdapter),
    ClaudeCode(ClaudeCodeAdapter),
    Kilo(KiloAdapter),
}

impl SelectedRuntimeAdapter {
    fn initialize(&mut self) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.initialize(),
            Self::Codex(a) => a.initialize(),
            Self::ClaudeCode(a) => a.initialize(),
            Self::Kilo(a) => a.initialize(),
        }
    }

    fn prepare(
        &mut self,
        task_id: Uuid,
        worktree: &std::path::Path,
    ) -> std::result::Result<(), RuntimeError> {
        match self {
            Self::OpenCode(a) => a.prepare(task_id, worktree),
            Self::Codex(a) => a.prepare(task_id, worktree),
            Self::ClaudeCode(a) => a.prepare(task_id, worktree),
            Self::Kilo(a) => a.prepare(task_id, worktree),
        }
    }

    fn execute(
        &mut self,
        input: ExecutionInput,
    ) -> std::result::Result<crate::adapters::runtime::ExecutionReport, RuntimeError> {
        match self {
            Self::OpenCode(a) => a.execute(input),
            Self::Codex(a) => a.execute(input),
            Self::ClaudeCode(a) => a.execute(input),
            Self::Kilo(a) => a.execute(input),
        }
    }

    fn execute_interactive<F>(
        &mut self,
        input: &ExecutionInput,
        on_event: F,
    ) -> std::result::Result<InteractiveExecutionResult, RuntimeError>
    where
        F: FnMut(InteractiveAdapterEvent) -> std::result::Result<(), String>,
    {
        match self {
            Self::OpenCode(a) => a.execute_interactive(input, on_event),
            Self::Codex(a) => a.execute_interactive(input, on_event),
            Self::ClaudeCode(a) => a.execute_interactive(input, on_event),
            Self::Kilo(a) => a.execute_interactive(input, on_event),
        }
    }
}

#[derive(Debug, Clone)]
struct ClassifiedRuntimeError {
    code: String,
    category: String,
    message: String,
    recoverable: bool,
    retryable: bool,
    rate_limited: bool,
}

impl Registry {
    fn has_model_flag(args: &[String], short_alias: bool) -> bool {
        args.iter().any(|arg| {
            arg == "--model" || arg.starts_with("--model=") || (short_alias && arg == "-m")
        })
    }

    fn runtime_start_flags(runtime: &ProjectRuntimeConfig) -> Vec<String> {
        match runtime.adapter_name.as_str() {
            "codex" => {
                let mut flags = if runtime.args.is_empty() {
                    CodexConfig::default().base.args
                } else {
                    runtime.args.clone()
                };

                if let Some(model) = runtime.model.as_ref() {
                    if !Self::has_model_flag(&flags, false) {
                        flags.extend(["--model".to_string(), model.clone()]);
                    }
                }

                flags
            }
            "claude-code" => {
                let mut flags = if runtime.args.is_empty() {
                    ClaudeCodeConfig::default().base.args
                } else {
                    runtime.args.clone()
                };

                if let Some(model) = runtime.model.as_ref() {
                    if !Self::has_model_flag(&flags, false) {
                        flags.extend(["--model".to_string(), model.clone()]);
                    }
                }

                flags
            }
            "opencode" | "kilo" => {
                let mut flags = runtime.args.clone();
                let is_opencode_binary = PathBuf::from(&runtime.binary_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| {
                        let lower = s.to_ascii_lowercase();
                        lower.contains("opencode") || lower.contains("kilo")
                    });

                if is_opencode_binary {
                    let mut with_start = vec!["run".to_string()];
                    if let Some(model) = runtime.model.as_ref() {
                        if !Self::has_model_flag(&flags, true) {
                            with_start.extend(["--model".to_string(), model.clone()]);
                        }
                    }
                    with_start.append(&mut flags);
                    with_start
                } else {
                    flags
                }
            }
            _ => runtime.args.clone(),
        }
    }

    fn classify_runtime_error(
        code: &str,
        message: &str,
        recoverable: bool,
        stdout: &str,
        stderr: &str,
    ) -> ClassifiedRuntimeError {
        let combined = format!("{code} {message} {stdout} {stderr}").to_ascii_lowercase();
        let rate_limited = combined.contains("rate limit")
            || combined.contains("rate_limit")
            || combined.contains("too many requests")
            || combined.contains(" 429")
            || combined.contains("status 429")
            || combined.contains("http 429");

        let category = if rate_limited {
            "rate_limit"
        } else if code == "timeout" {
            "timeout"
        } else if code == "checkpoints_incomplete" {
            "checkpoint_incomplete"
        } else if matches!(
            code,
            "binary_not_found" | "health_check_failed" | "missing_args" | "worktree_not_found"
        ) {
            "configuration"
        } else if code.contains("initialize") || code.contains("prepare") {
            "runtime_setup"
        } else {
            "runtime_execution"
        };

        let effective_recoverable = recoverable || rate_limited || code == "checkpoints_incomplete";
        let retryable = effective_recoverable
            && !matches!(
                code,
                "binary_not_found"
                    | "health_check_failed"
                    | "missing_args"
                    | "worktree_not_found"
                    | "checkpoints_incomplete"
            );

        ClassifiedRuntimeError {
            code: code.to_string(),
            category: category.to_string(),
            message: message.to_string(),
            recoverable: effective_recoverable,
            retryable,
            rate_limited,
        }
    }

    fn detect_runtime_output_failure(stdout: &str, stderr: &str) -> Option<(String, String, bool)> {
        let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();

        if combined.contains("incorrect api key")
            || combined.contains("invalid api key")
            || combined.contains("authentication failed")
            || combined.contains("unauthorized")
            || combined.contains("forbidden")
        {
            return Some((
                "runtime_auth_failed".to_string(),
                "Runtime authentication failed; inspect runtime stderr for details".to_string(),
                false,
            ));
        }

        if combined.contains("rate limit")
            || combined.contains("too many requests")
            || combined.contains("http 429")
            || combined.contains("status 429")
            || combined.contains(" 429")
        {
            return Some((
                "runtime_rate_limited".to_string(),
                "Runtime reported rate limiting; retry recommended".to_string(),
                true,
            ));
        }

        None
    }

    fn emit_runtime_error_classified(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        adapter_name: &str,
        classified: &ClassifiedRuntimeError,
        origin: &'static str,
    ) -> Result<()> {
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::RuntimeErrorClassified {
                    attempt_id,
                    adapter_name: adapter_name.to_string(),
                    code: classified.code.clone(),
                    category: classified.category.clone(),
                    message: classified.message.clone(),
                    recoverable: classified.recoverable,
                    retryable: classified.retryable,
                    rate_limited: classified.rate_limited,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let mut err = HivemindError::runtime(
            format!("runtime_{}", classified.code),
            classified.message.clone(),
            origin,
        )
        .recoverable(classified.recoverable)
        .with_context("adapter", adapter_name.to_string())
        .with_context("runtime_error_code", classified.code.clone())
        .with_context("runtime_error_category", classified.category.clone());

        if classified.rate_limited {
            err = err.with_hint(
                "Rate limiting detected. Hivemind will schedule a retry and may switch to a backup runtime if configured.",
            );
        }

        self.record_error_event(&err, corr_attempt);
        Ok(())
    }

    fn select_runtime_backup(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        primary: &ProjectRuntimeConfig,
    ) -> Option<ProjectRuntimeConfig> {
        if let Ok(candidate) = Self::effective_runtime_for_task(
            state,
            flow,
            task_id,
            RuntimeRole::Validator,
            "registry:tick_flow",
        ) {
            if candidate != *primary {
                return Some(candidate);
            }
        }

        match primary.adapter_name.as_str() {
            "opencode" => {
                let mut fallback = primary.clone();
                fallback.adapter_name = "kilo".to_string();
                Some(fallback)
            }
            "kilo" => {
                let mut fallback = primary.clone();
                fallback.adapter_name = "opencode".to_string();
                Some(fallback)
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_runtime_recovery(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        from_runtime: &ProjectRuntimeConfig,
        classified: &ClassifiedRuntimeError,
        origin: &'static str,
    ) -> Result<()> {
        let backup = Self::select_runtime_backup(state, flow, task_id, from_runtime);
        let target_runtime = if classified.rate_limited {
            backup.unwrap_or_else(|| from_runtime.clone())
        } else {
            from_runtime.clone()
        };

        let strategy = if target_runtime.adapter_name == from_runtime.adapter_name {
            "same_runtime_retry"
        } else {
            "fallback_runtime"
        };
        let backoff_ms = if classified.rate_limited { 1_000 } else { 250 };

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );
        self.append_event(
            Event::new(
                EventPayload::RuntimeRecoveryScheduled {
                    attempt_id,
                    from_adapter: from_runtime.adapter_name.clone(),
                    to_adapter: target_runtime.adapter_name.clone(),
                    strategy: strategy.to_string(),
                    reason: classified.code.clone(),
                    backoff_ms,
                },
                corr_attempt,
            ),
            origin,
        )?;

        if target_runtime.adapter_name != from_runtime.adapter_name {
            let corr_task = CorrelationIds::for_graph_flow_task(
                flow.project_id,
                flow.graph_id,
                flow.id,
                task_id,
            );
            self.append_event(
                Event::new(
                    EventPayload::TaskRuntimeRoleConfigured {
                        task_id,
                        role: RuntimeRole::Worker,
                        adapter_name: target_runtime.adapter_name,
                        binary_path: target_runtime.binary_path,
                        model: target_runtime.model,
                        args: target_runtime.args,
                        env: target_runtime.env,
                        timeout_ms: target_runtime.timeout_ms,
                    },
                    corr_task,
                ),
                origin,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_runtime_failure(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        runtime: &ProjectRuntimeConfig,
        next_attempt_number: u32,
        max_attempts: u32,
        failure_code: &str,
        failure_message: &str,
        recoverable: bool,
        stdout: &str,
        stderr: &str,
        origin: &'static str,
    ) -> Result<()> {
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        let reason = format!("{failure_code}: {failure_message}");
        self.append_event(
            Event::new(
                EventPayload::RuntimeTerminated { attempt_id, reason },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let classified = Self::classify_runtime_error(
            failure_code,
            failure_message,
            recoverable,
            stdout,
            stderr,
        );
        self.emit_runtime_error_classified(
            flow,
            task_id,
            attempt_id,
            &runtime.adapter_name,
            &classified,
            origin,
        )?;

        if failure_code == "checkpoints_incomplete" {
            return Ok(());
        }

        self.fail_running_attempt(flow, task_id, attempt_id, failure_code, origin)?;

        let should_retry = classified.retryable
            && next_attempt_number < max_attempts
            && (classified.rate_limited
                || matches!(
                    classified.code.as_str(),
                    "timeout" | "wait_failed" | "stdin_write_failed"
                ));

        if should_retry {
            self.schedule_runtime_recovery(
                state,
                flow,
                task_id,
                attempt_id,
                runtime,
                &classified,
                origin,
            )?;
            if let Err(err) = self.retry_task(&task_id.to_string(), false, RetryMode::Continue) {
                self.record_error_event(&err, corr_attempt);
            }
        }

        Ok(())
    }

    fn format_checkpoint_commit_message(spec: &CheckpointCommitSpec<'_>) -> String {
        let mut message = String::new();
        let _ = writeln!(message, "hivemind(checkpoint): {}", spec.checkpoint_id);
        let _ = writeln!(message);
        let _ = writeln!(message, "Flow: {}", spec.flow_id);
        let _ = writeln!(message, "Task: {}", spec.task_id);
        let _ = writeln!(message, "Attempt: {}", spec.attempt_id);
        let _ = writeln!(message, "Checkpoint: {}/{}", spec.order, spec.total);
        let _ = writeln!(message, "Schema: checkpoint-v1");
        if let Some(summary) = spec.summary {
            let _ = writeln!(message);
            let _ = writeln!(message, "Summary:");
            let _ = writeln!(message, "{summary}");
        }
        let _ = writeln!(message);
        let _ = writeln!(message, "---");
        let _ = writeln!(
            message,
            "Generated-by: Hivemind {}",
            env!("CARGO_PKG_VERSION")
        );
        message
    }

    fn create_checkpoint_commit(
        worktree_path: &Path,
        spec: &CheckpointCommitSpec<'_>,
        origin: &'static str,
    ) -> Result<String> {
        let add = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["add", "-A"])
            .output()
            .map_err(|e| HivemindError::git("git_add_failed", e.to_string(), origin))?;
        if !add.status.success() {
            return Err(HivemindError::git(
                "git_add_failed",
                String::from_utf8_lossy(&add.stderr).to_string(),
                origin,
            ));
        }

        let message = Self::format_checkpoint_commit_message(spec);
        let commit = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "--allow-empty",
                "-m",
                &message,
            ])
            .output()
            .map_err(|e| HivemindError::git("git_commit_failed", e.to_string(), origin))?;
        if !commit.status.success() {
            return Err(HivemindError::git(
                "git_commit_failed",
                String::from_utf8_lossy(&commit.stderr).to_string(),
                origin,
            ));
        }

        let head = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| HivemindError::git("git_rev_parse_failed", e.to_string(), origin))?;
        if !head.status.success() {
            return Err(HivemindError::git(
                "git_rev_parse_failed",
                String::from_utf8_lossy(&head.stderr).to_string(),
                origin,
            ));
        }
        Ok(String::from_utf8_lossy(&head.stdout).trim().to_string())
    }

    fn checkout_and_clean_worktree(
        worktree_path: &Path,
        branch: &str,
        base: &str,
        origin: &'static str,
    ) -> Result<()> {
        let checkout = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["checkout", "-f", "-B", branch, base])
            .output();
        if !checkout.as_ref().is_ok_and(|o| o.status.success()) {
            return Err(HivemindError::git(
                "git_checkout_failed",
                checkout.map_or_else(
                    |e| e.to_string(),
                    |o| String::from_utf8_lossy(&o.stderr).to_string(),
                ),
                origin,
            ));
        }

        let clean = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["clean", "-fdx"])
            .output();
        if !clean.as_ref().is_ok_and(|o| o.status.success()) {
            return Err(HivemindError::git(
                "git_clean_failed",
                clean.map_or_else(
                    |e| e.to_string(),
                    |o| String::from_utf8_lossy(&o.stderr).to_string(),
                ),
                origin,
            ));
        }

        Ok(())
    }

    fn append_event(&self, event: Event, origin: &'static str) -> Result<()> {
        self.store
            .append(event)
            .map(|_| ())
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))
    }

    fn projected_runtime_event_payload(
        attempt_id: Uuid,
        observation: ProjectedRuntimeObservation,
    ) -> EventPayload {
        match observation {
            ProjectedRuntimeObservation::CommandObserved { stream, command } => {
                EventPayload::RuntimeCommandObserved {
                    attempt_id,
                    stream,
                    command,
                }
            }
            ProjectedRuntimeObservation::ToolCallObserved {
                stream,
                tool_name,
                details,
            } => EventPayload::RuntimeToolCallObserved {
                attempt_id,
                stream,
                tool_name,
                details,
            },
            ProjectedRuntimeObservation::TodoSnapshotUpdated { stream, items } => {
                EventPayload::RuntimeTodoSnapshotUpdated {
                    attempt_id,
                    stream,
                    items,
                }
            }
            ProjectedRuntimeObservation::NarrativeOutputObserved { stream, content } => {
                EventPayload::RuntimeNarrativeOutputObserved {
                    attempt_id,
                    stream,
                    content,
                }
            }
        }
    }

    fn append_projected_runtime_observations(
        &self,
        attempt_id: Uuid,
        correlation: &CorrelationIds,
        observations: Vec<ProjectedRuntimeObservation>,
        origin: &'static str,
    ) -> Result<()> {
        for observation in observations {
            self.append_event(
                Event::new(
                    Self::projected_runtime_event_payload(attempt_id, observation),
                    correlation.clone(),
                ),
                origin,
            )?;
        }
        Ok(())
    }

    fn acquire_flow_integration_lock(
        &self,
        flow_id: Uuid,
        origin: &'static str,
    ) -> Result<std::fs::File> {
        let lock_dir = self.config.data_dir.join("locks");
        fs::create_dir_all(&lock_dir)
            .map_err(|e| HivemindError::system("create_dir_failed", e.to_string(), origin))?;

        let lock_path = lock_dir.join(format!("flow_integration_{flow_id}.lock"));
        for attempt in 0..5 {
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| HivemindError::system("lock_open_failed", e.to_string(), origin))?;

            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(err) => {
                    if attempt < 4 {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    return Err(HivemindError::user(
                        "integration_in_progress",
                        "Another integration operation is already in progress for this flow",
                        origin,
                    )
                    .with_context("flow_id", flow_id.to_string())
                    .with_context("lock_error", err.to_string()));
                }
            }
        }

        Err(HivemindError::user(
            "integration_in_progress",
            "Another integration operation is already in progress for this flow",
            origin,
        )
        .with_context("flow_id", flow_id.to_string())
        .with_context("lock_error", "retry_exhausted"))
    }

    fn resolve_git_ref(repo_path: &Path, reference: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", reference])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            return None;
        }
        Some(sha)
    }

    fn resolve_task_frozen_commit_sha(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
    ) -> Option<String> {
        let manager = Self::worktree_manager_for_flow(flow, state).ok()?;
        let branch_ref = format!("refs/heads/exec/{}/{task_id}", flow.id);
        Self::resolve_git_ref(manager.repo_path(), &branch_ref).or_else(|| {
            let status = manager.inspect(flow.id, task_id).ok()?;
            if !status.is_worktree {
                return None;
            }
            status
                .head_commit
                .or_else(|| Self::resolve_git_ref(&status.path, "HEAD"))
        })
    }

    pub fn list_graphs(&self, project_id_or_name: Option<&str>) -> Result<Vec<TaskGraph>> {
        let project_filter = match project_id_or_name {
            Some(id_or_name) => Some(self.get_project(id_or_name)?.id),
            None => None,
        };

        let state = self.state()?;
        let mut graphs: Vec<_> = state
            .graphs
            .into_values()
            .filter(|g| project_filter.is_none_or(|pid| g.project_id == pid))
            .collect();
        graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        graphs.reverse();
        Ok(graphs)
    }

    fn emit_task_execution_frozen(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        commit_sha: Option<String>,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::TaskExecutionFrozen {
                    flow_id: flow.id,
                    task_id,
                    commit_sha,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            ),
            origin,
        )
    }

    fn emit_integration_lock_acquired(
        &self,
        flow: &TaskFlow,
        operation: &str,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::FlowIntegrationLockAcquired {
                    flow_id: flow.id,
                    operation: operation.to_string(),
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            origin,
        )
    }

    fn emit_merge_conflict(
        &self,
        flow: &TaskFlow,
        task_id: Option<Uuid>,
        details: String,
        origin: &'static str,
    ) -> Result<()> {
        self.append_event(
            Event::new(
                EventPayload::MergeConflictDetected {
                    flow_id: flow.id,
                    task_id,
                    details,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            origin,
        )
    }

    fn attempt_runtime_outcome(&self, attempt_id: Uuid) -> Result<(Option<i32>, Option<String>)> {
        let filter = EventFilter {
            attempt_id: Some(attempt_id),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;

        let mut exit_code: Option<i32> = None;
        let mut terminated: Option<String> = None;
        for ev in events {
            match ev.payload {
                EventPayload::RuntimeExited { exit_code: ec, .. } => {
                    exit_code = Some(ec);
                }
                EventPayload::RuntimeTerminated { reason, .. } => {
                    terminated = Some(reason);
                }
                _ => {}
            }
        }

        if exit_code.is_none() && terminated.is_none() {
            return Ok((None, None));
        }
        Ok((exit_code, terminated))
    }

    #[allow(
        clippy::type_complexity,
        clippy::too_many_lines,
        clippy::unnecessary_wraps
    )]
    fn build_retry_context(
        &self,
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_number: u32,
        max_attempts: u32,
        _origin: &'static str,
    ) -> Result<(
        String,
        Vec<AttemptSummary>,
        Vec<Uuid>,
        Vec<String>,
        Vec<String>,
        Option<i32>,
        Option<String>,
    )> {
        let mut attempts: Vec<AttemptState> = state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow.id && a.task_id == task_id)
            .filter(|a| a.attempt_number < attempt_number)
            .cloned()
            .collect();
        attempts.sort_by_key(|a| a.attempt_number);

        let prior_attempt_ids: Vec<Uuid> = attempts.iter().map(|a| a.id).collect();

        let mut prior_attempts: Vec<AttemptSummary> = Vec::new();
        for prior in &attempts {
            let (exit_code, terminated_reason) = self
                .attempt_runtime_outcome(prior.id)
                .unwrap_or((None, None));

            let required_failed: Vec<String> = prior
                .check_results
                .iter()
                .filter(|r| r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            let optional_failed: Vec<String> = prior
                .check_results
                .iter()
                .filter(|r| !r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            let incomplete_checkpoints: Vec<String> = prior
                .checkpoints
                .iter()
                .filter(|cp| cp.state != AttemptCheckpointState::Completed)
                .map(|cp| cp.checkpoint_id.clone())
                .collect();

            let mut summary = String::new();
            if let Some(diff_id) = prior.diff_id {
                if let Ok(artifact) = self.read_diff_artifact(diff_id) {
                    let _ = write!(summary, "change_count={} ", artifact.diff.change_count());
                }
            }

            if required_failed.is_empty() && optional_failed.is_empty() {
                if let Some(ec) = exit_code {
                    let _ = write!(summary, "runtime_exit_code={ec} ");
                }
                if let Some(ref reason) = terminated_reason {
                    let _ = write!(summary, "runtime_terminated={reason} ");
                }
            }

            if !required_failed.is_empty() {
                let _ = write!(
                    summary,
                    "required_checks_failed={} ",
                    required_failed.join(", ")
                );
            }
            if !optional_failed.is_empty() {
                let _ = write!(
                    summary,
                    "optional_checks_failed={} ",
                    optional_failed.join(", ")
                );
            }
            if !incomplete_checkpoints.is_empty() {
                let _ = write!(
                    summary,
                    "pending_checkpoints={} ",
                    incomplete_checkpoints.join(", ")
                );
            }

            if summary.trim().is_empty() {
                summary = "no recorded outcomes".to_string();
            }

            let failure_reason = if !required_failed.is_empty() {
                Some(format!(
                    "Required checks failed: {}",
                    required_failed.join(", ")
                ))
            } else if !optional_failed.is_empty() {
                Some(format!(
                    "Optional checks failed: {}",
                    optional_failed.join(", ")
                ))
            } else if !incomplete_checkpoints.is_empty() {
                Some(format!(
                    "Checkpoints incomplete: {}",
                    incomplete_checkpoints.join(", ")
                ))
            } else if let Some(ec) = exit_code {
                if ec != 0 {
                    Some(format!("Runtime exited with code {ec}"))
                } else {
                    None
                }
            } else if terminated_reason.is_some() {
                Some("Runtime terminated".to_string())
            } else {
                None
            };

            prior_attempts.push(AttemptSummary {
                attempt_number: prior.attempt_number,
                summary: summary.trim().to_string(),
                failure_reason,
            });
        }

        let latest = attempts.last();
        let mut required_failures: Vec<String> = Vec::new();
        let mut optional_failures: Vec<String> = Vec::new();
        let mut incomplete_checkpoints: Vec<String> = Vec::new();
        let mut exit_code: Option<i32> = None;
        #[allow(clippy::useless_let_if_seq, clippy::option_if_let_else)]
        let terminated_reason = if let Some(last) = latest {
            required_failures = last
                .check_results
                .iter()
                .filter(|r| r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            optional_failures = last
                .check_results
                .iter()
                .filter(|r| !r.required && !r.passed)
                .map(|r| r.name.clone())
                .collect();
            incomplete_checkpoints = last
                .checkpoints
                .iter()
                .filter(|cp| cp.state != AttemptCheckpointState::Completed)
                .map(|cp| cp.checkpoint_id.clone())
                .collect();

            let (ec, term) = self
                .attempt_runtime_outcome(last.id)
                .unwrap_or((None, None));
            exit_code = ec;
            term
        } else {
            None
        };

        #[allow(clippy::items_after_statements)]
        fn truncate(s: &str, max_len: usize) -> String {
            if s.len() <= max_len {
                return s.to_string();
            }
            s.chars().take(max_len).collect()
        }

        let mut ctx = String::new();
        let _ = writeln!(
            ctx,
            "Retry attempt {attempt_number}/{max_attempts} for task {task_id}"
        );

        if let Some(last) = latest {
            let _ = writeln!(ctx, "Previous attempt: {}", last.id);

            if !required_failures.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Required check failures (must fix): {}",
                    required_failures.join(", ")
                );
                for r in last
                    .check_results
                    .iter()
                    .filter(|r| r.required && !r.passed)
                {
                    let _ = writeln!(ctx, "--- Check: {}", r.name);
                    let _ = writeln!(ctx, "exit_code={}", r.exit_code);
                    let _ = writeln!(ctx, "output:\n{}", truncate(&r.output, 2000));
                }
            }

            if !optional_failures.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Optional check failures: {}",
                    optional_failures.join(", ")
                );
            }

            if let Some(ec) = exit_code {
                let _ = writeln!(ctx, "Runtime exit code: {ec}");
            }
            if let Some(ref reason) = terminated_reason {
                let _ = writeln!(ctx, "Runtime terminated: {reason}");
            }
            if !incomplete_checkpoints.is_empty() {
                let _ = writeln!(
                    ctx,
                    "Incomplete checkpoints from prior attempt: {}",
                    incomplete_checkpoints.join(", ")
                );
            }

            if let Some(diff_id) = last.diff_id {
                if let Ok(artifact) = self.read_diff_artifact(diff_id) {
                    let created: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Created)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();
                    let modified: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Modified)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();
                    let deleted: Vec<String> = artifact
                        .diff
                        .changes
                        .iter()
                        .filter(|c| c.change_type == ChangeType::Deleted)
                        .map(|c| c.path.to_string_lossy().to_string())
                        .collect();

                    let _ = writeln!(
                        ctx,
                        "Filesystem changes observed (from diff): change_count={} created={} modified={} deleted={}",
                        artifact.diff.change_count(),
                        created.len(),
                        modified.len(),
                        deleted.len()
                    );
                    if !created.is_empty() {
                        let _ = writeln!(ctx, "Created:\n{}", created.join("\n"));
                    }
                    if !modified.is_empty() {
                        let _ = writeln!(ctx, "Modified:\n{}", modified.join("\n"));
                    }
                    if !deleted.is_empty() {
                        let _ = writeln!(ctx, "Deleted:\n{}", deleted.join("\n"));
                    }
                }
            }
        }

        Ok((
            ctx,
            prior_attempts,
            prior_attempt_ids,
            required_failures,
            optional_failures,
            exit_code,
            terminated_reason,
        ))
    }

    fn record_error_event(&self, err: &HivemindError, correlation: CorrelationIds) {
        let _ = self.store.append(Event::new(
            EventPayload::ErrorOccurred { error: err.clone() },
            correlation,
        ));
    }

    fn flow_for_task(state: &AppState, task_id: Uuid, origin: &'static str) -> Result<TaskFlow> {
        state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&task_id))
            .max_by_key(|f| (f.updated_at, f.id))
            .cloned()
            .ok_or_else(|| {
                HivemindError::user("task_not_in_flow", "Task is not part of any flow", origin)
            })
    }

    fn inspect_task_worktree(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let manager = Self::worktree_manager_for_flow(flow, state)?;
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if !status.is_worktree {
            return Err(HivemindError::user(
                "worktree_not_found",
                "Worktree not found for task",
                origin,
            ));
        }
        Ok(status)
    }

    fn resolve_latest_attempt_without_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_none())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for running task",
                    origin,
                )
            })
    }

    fn normalized_checkpoint_ids(raw: &[String]) -> Vec<String> {
        let mut ids = Vec::new();
        let mut seen = HashSet::new();

        for candidate in raw {
            let trimmed = candidate.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                ids.push(trimmed.to_string());
            }
        }

        if ids.is_empty() {
            ids.push("checkpoint-1".to_string());
        }

        ids
    }

    fn checkpoint_order(checkpoint_ids: &[String], checkpoint_id: &str) -> Option<(u32, u32)> {
        let idx = checkpoint_ids.iter().position(|id| id == checkpoint_id)?;
        let order = u32::try_from(idx.saturating_add(1)).ok()?;
        let total = u32::try_from(checkpoint_ids.len()).ok()?;
        Some((order, total))
    }

    fn fail_running_attempt(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt_id: Uuid,
        reason: &str,
        origin: &'static str,
    ) -> Result<()> {
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    from: TaskExecState::Running,
                    to: TaskExecState::Failed,
                },
                corr_task,
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionFailed {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt_id),
                    reason: Some(reason.to_string()),
                },
                corr_attempt,
            ),
            origin,
        )
    }

    fn resolve_latest_attempt_with_diff(
        state: &AppState,
        flow_id: Uuid,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<AttemptState> {
        state
            .attempts
            .values()
            .filter(|a| a.flow_id == flow_id && a.task_id == task_id)
            .filter(|a| a.diff_id.is_some())
            .max_by_key(|a| a.started_at)
            .cloned()
            .ok_or_else(|| {
                HivemindError::system(
                    "attempt_not_found",
                    "Attempt not found for verifying task",
                    origin,
                )
            })
    }

    #[allow(clippy::too_many_lines)]
    fn process_verifying_task(&self, flow_id: &str, task_id: Uuid) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Ok(flow);
        }

        let state = self.state()?;
        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
        })?;

        let origin = "registry:tick_flow";
        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Verifying {
            return Ok(flow);
        }

        let attempt = Self::resolve_latest_attempt_with_diff(&state, flow.id, task_id, origin)?;
        let diff_id = attempt.diff_id.ok_or_else(|| {
            HivemindError::system("diff_not_found", "Diff not found for attempt", origin)
        })?;
        let artifact = self.read_diff_artifact(diff_id)?;

        let baseline_id = attempt.baseline_id.ok_or_else(|| {
            HivemindError::system(
                "baseline_not_found",
                "Baseline not found for attempt",
                origin,
            )
        })?;
        let baseline = self.read_baseline_artifact(baseline_id)?;

        let worktree_status = Self::inspect_task_worktree(&flow, &state, task_id, origin)?;

        let task = graph.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;

        let mut verification = if let Some(scope) = &task.scope {
            let (commits_created, branches_created) =
                Self::detect_git_operations(&worktree_status.path, &baseline, attempt.id);

            ScopeEnforcer::new(scope.clone()).verify_all(
                &artifact.diff,
                commits_created,
                branches_created,
                task_id,
                attempt.id,
            )
        } else {
            VerificationResult::pass(task_id, attempt.id)
        };

        if let Some(scope) = &task.scope {
            let repo_violations =
                Self::verify_repository_scope(scope, &flow, &state, task_id, origin);
            if !repo_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(repo_violations);
            }

            let ambient_violations =
                self.verify_scope_environment_baseline(&flow, &state, task_id, attempt.id, origin);
            if !ambient_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(ambient_violations);
            }

            let traced_violations =
                self.verify_scope_trace_writes(&flow, &state, task_id, attempt.id, origin);
            if !traced_violations.is_empty() {
                verification.passed = false;
                verification.violations.extend(traced_violations);
            }
        }

        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);

        if !verification.passed {
            if let Some(scope) = &task.scope {
                self.append_event(
                    Event::new(
                        EventPayload::ScopeViolationDetected {
                            flow_id: flow.id,
                            task_id,
                            attempt_id: attempt.id,
                            verification_id: verification.id,
                            verified_at: verification.verified_at,
                            scope: scope.clone(),
                            violations: verification.violations.clone(),
                        },
                        CorrelationIds::for_graph_flow_task_attempt(
                            flow.project_id,
                            flow.graph_id,
                            flow.id,
                            task_id,
                            attempt.id,
                        ),
                    ),
                    origin,
                )?;
            }

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        from: TaskExecState::Verifying,
                        to: TaskExecState::Failed,
                    },
                    corr_task,
                ),
                origin,
            )?;

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        reason: Some("scope_violation".to_string()),
                    },
                    CorrelationIds::for_graph_flow_task_attempt(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        task_id,
                        attempt.id,
                    ),
                ),
                origin,
            )?;

            let violations = verification
                .violations
                .iter()
                .map(|v| {
                    let path = v.path.as_deref().unwrap_or("-");
                    format!("{:?}: {path}: {}", v.violation_type, v.description)
                })
                .collect::<Vec<_>>()
                .join("\n");

            return Err(HivemindError::scope(
                "scope_violation",
                format!("Scope violation detected:\n{violations}"),
                origin,
            )
            .with_hint(format!(
                "Worktree preserved at {}",
                worktree_status.path.display()
            )));
        }

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt.id,
        );

        if let Some(scope) = &task.scope {
            self.append_event(
                Event::new(
                    EventPayload::ScopeValidated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        verification_id: verification.id,
                        verified_at: verification.verified_at,
                        scope: scope.clone(),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt.id.to_string())
            .join("checks");
        let _ = fs::create_dir_all(&target_dir);

        let mut results = Vec::new();
        for check in &task.criteria.checks {
            self.append_event(
                Event::new(
                    EventPayload::CheckStarted {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        check_name: check.name.clone(),
                        required: check.required,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;

            let started = Instant::now();
            let (exit_code, combined) = match Self::run_check_command(
                &worktree_status.path,
                &target_dir,
                &check.command,
                check.timeout_ms,
            ) {
                Ok((exit_code, output, _timed_out)) => (exit_code, output),
                Err(e) => (127, e.to_string()),
            };
            let duration_ms =
                u64::try_from(started.elapsed().as_millis().min(u128::from(u64::MAX)))
                    .unwrap_or(u64::MAX);
            let passed = exit_code == 0;

            self.append_event(
                Event::new(
                    EventPayload::CheckCompleted {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        check_name: check.name.clone(),
                        passed,
                        exit_code,
                        output: combined.clone(),
                        duration_ms,
                        required: check.required,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;

            results.push((check.name.clone(), check.required, passed));
        }

        let required_failed = results
            .iter()
            .any(|(_, required, passed)| *required && !*passed);

        if !required_failed {
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        from: TaskExecState::Verifying,
                        to: TaskExecState::Success,
                    },
                    corr_task,
                ),
                origin,
            )?;

            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionSucceeded {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                    },
                    corr_attempt,
                ),
                origin,
            )?;

            let frozen_commit_sha = Self::resolve_task_frozen_commit_sha(&flow, &state, task_id);
            self.emit_task_execution_frozen(&flow, task_id, frozen_commit_sha, origin)?;

            if let Ok(managers) =
                Self::worktree_managers_for_flow(&flow, &state, "registry:tick_flow")
            {
                for (_repo_name, manager) in managers {
                    if manager.config().cleanup_on_success {
                        if let Ok(status) = manager.inspect(flow.id, task_id) {
                            if status.is_worktree {
                                let _ = manager.remove(&status.path);
                            }
                        }
                    }
                }
            }

            let updated = self.get_flow(flow_id)?;
            let all_success = updated
                .task_executions
                .values()
                .all(|e| e.state == TaskExecState::Success);
            if all_success {
                let event = Event::new(
                    EventPayload::TaskFlowCompleted {
                        flow_id: updated.id,
                    },
                    CorrelationIds::for_graph_flow(
                        updated.project_id,
                        updated.graph_id,
                        updated.id,
                    ),
                );
                self.append_event(event, origin)?;
                self.maybe_autostart_dependent_flows(updated.id)?;
            }

            return self.get_flow(flow_id);
        }

        let max_retries = task.retry_policy.max_retries;
        let max_attempts = max_retries.saturating_add(1);
        let can_retry = exec.attempt_count < max_attempts;
        let to = if can_retry {
            TaskExecState::Retry
        } else {
            TaskExecState::Failed
        };

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt.id),
                    from: TaskExecState::Verifying,
                    to,
                },
                corr_task,
            ),
            origin,
        )?;

        if matches!(to, TaskExecState::Retry | TaskExecState::Failed) {
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: Some(attempt.id),
                        reason: Some("required_checks_failed".to_string()),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        let failures = results
            .into_iter()
            .filter(|(_, required, passed)| *required && !*passed)
            .map(|(name, _, _)| name)
            .collect::<Vec<_>>()
            .join(", ");

        let err = HivemindError::verification(
            "required_checks_failed",
            format!("Required checks failed: {failures}"),
            origin,
        )
        .with_hint(format!(
            "View check outputs via `hivemind verify results {}`. Worktree preserved at {}",
            attempt.id,
            worktree_status.path.display()
        ));

        self.record_error_event(&err, corr_attempt);

        Err(err)
    }

    pub fn verify_run(&self, task_id: &str) -> Result<TaskFlow> {
        let origin = "registry:verify_run";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;
        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Verifying {
            return Err(HivemindError::user(
                "task_not_verifying",
                "Task is not in verifying state",
                origin,
            )
            .with_hint(
                "Complete the task execution first, or run `hivemind flow tick <flow-id>`",
            ));
        }

        self.process_verifying_task(&flow.id.to_string(), id)
    }

    fn run_check_command(
        workdir: &Path,
        cargo_target_dir: &Path,
        command: &str,
        timeout_ms: Option<u64>,
    ) -> std::io::Result<(i32, String, bool)> {
        let started = Instant::now();

        let mut cmd = std::process::Command::new("sh");
        cmd.current_dir(workdir)
            .env("CARGO_TARGET_DIR", cargo_target_dir)
            .args(["-lc", command]);

        if let Some(timeout_ms) = timeout_ms {
            let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;

            let mut out_buf = Vec::new();
            let mut err_buf = Vec::new();

            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let out_handle = std::thread::spawn(move || {
                if let Some(mut stdout) = stdout {
                    let _ = stdout.read_to_end(&mut out_buf);
                }
                out_buf
            });
            let err_handle = std::thread::spawn(move || {
                if let Some(mut stderr) = stderr {
                    let _ = stderr.read_to_end(&mut err_buf);
                }
                err_buf
            });

            let timeout = Duration::from_millis(timeout_ms);
            let mut timed_out = false;
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if started.elapsed() >= timeout {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait()?;
                }
                std::thread::sleep(Duration::from_millis(10));
            };

            let stdout_buf = out_handle.join().unwrap_or_default();
            let stderr_buf = err_handle.join().unwrap_or_default();

            let mut combined = String::new();
            if timed_out {
                let _ = writeln!(combined, "timed out after {timeout_ms}ms");
            }
            combined.push_str(&String::from_utf8_lossy(&stdout_buf));
            if !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&String::from_utf8_lossy(&stderr_buf));

            let exit_code = if timed_out {
                124
            } else {
                status.code().unwrap_or(-1)
            };

            return Ok((exit_code, combined, timed_out));
        }

        let out = cmd.output()?;
        let mut combined = String::new();
        combined.push_str(&String::from_utf8_lossy(&out.stdout));
        if !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        Ok((out.status.code().unwrap_or(-1), combined, false))
    }

    fn detect_git_operations(
        worktree_path: &Path,
        baseline: &Baseline,
        attempt_id: Uuid,
    ) -> (bool, bool) {
        let commits_created = Self::detect_commits_created(worktree_path, baseline, attempt_id);
        let branches_created = Self::detect_branches_created(worktree_path, baseline);
        (commits_created, branches_created)
    }

    fn detect_commits_created(worktree_path: &Path, baseline: &Baseline, attempt_id: Uuid) -> bool {
        let Some(base) = baseline.git_head.as_deref() else {
            return false;
        };

        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["log", "--format=%s", &format!("{base}..HEAD")])
            .output();

        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let mut subjects: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        subjects.retain(|s| s != &format!("hivemind checkpoint {attempt_id}"));
        subjects.retain(|s| !s.starts_with("hivemind(checkpoint): "));
        !subjects.is_empty()
    }

    fn detect_branches_created(worktree_path: &Path, baseline: &Baseline) -> bool {
        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["for-each-ref", "refs/heads", "--format=%(refname:short)"])
            .output();

        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let current: std::collections::HashSet<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let base: std::collections::HashSet<String> =
            baseline.git_branches.iter().cloned().collect();
        current.difference(&base).next().is_some()
    }

    fn parse_git_status_paths(worktree_path: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.strip_prefix("?? ")
                    .or_else(|| line.get(3..))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect()
    }

    fn verify_repository_scope(
        scope: &Scope,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        if scope.repositories.is_empty() {
            return Vec::new();
        }

        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return vec![crate::core::enforcement::ScopeViolation::filesystem(
                "<worktree>",
                "Repository scope verification failed: worktree missing",
            )];
        };

        let mut violations = Vec::new();
        for (repo_name, status) in worktrees {
            let changed_paths = Self::parse_git_status_paths(&status.path);
            if changed_paths.is_empty() {
                continue;
            }

            let allowed_mode = scope
                .repositories
                .iter()
                .find(|r| r.repo == repo_name || r.repo == status.path.to_string_lossy())
                .map(|r| r.mode);

            match allowed_mode {
                Some(RepoAccessMode::ReadWrite) => {}
                Some(RepoAccessMode::ReadOnly) => {
                    violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                        format!("{repo_name}/{}", changed_paths[0]),
                        format!("Repository '{repo_name}' is read-only in scope"),
                    ));
                }
                None => {
                    violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                        format!("{repo_name}/{}", changed_paths[0]),
                        format!("Repository '{repo_name}' is not declared in task scope"),
                    ));
                }
            }
        }
        violations
    }

    fn repo_git_head(path: &Path) -> Option<String> {
        std::process::Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|head| !head.is_empty())
    }

    fn repo_status_lines(path: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(["status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        lines.retain(|line| {
            let path = line
                .strip_prefix("?? ")
                .or_else(|| line.get(3..))
                .unwrap_or("")
                .trim();
            !path.starts_with(".hivemind/")
        });
        lines.sort();
        lines.dedup();
        lines
    }

    fn list_tmp_entries() -> Vec<String> {
        let Ok(entries) = fs::read_dir("/tmp") else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        names.dedup();
        names
    }

    fn write_scope_baseline_artifact(&self, artifact: &ScopeBaselineArtifact) -> Result<()> {
        fs::create_dir_all(self.scope_baselines_dir()).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        let bytes = serde_json::to_vec_pretty(artifact).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        fs::write(self.scope_baseline_path(artifact.attempt_id), bytes).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_scope_baseline_artifact",
            )
        })?;
        Ok(())
    }

    fn read_scope_baseline_artifact(&self, attempt_id: Uuid) -> Result<ScopeBaselineArtifact> {
        let bytes = fs::read(self.scope_baseline_path(attempt_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_scope_baseline_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_scope_baseline_artifact",
            )
        })
    }

    fn capture_scope_baseline_for_attempt(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        attempt_id: Uuid,
    ) -> Result<()> {
        let repo_snapshots = state
            .projects
            .get(&flow.project_id)
            .map(|project| {
                project
                    .repositories
                    .iter()
                    .map(|repo| ScopeRepoSnapshot {
                        repo_path: repo.path.clone(),
                        git_head: Self::repo_git_head(Path::new(&repo.path)),
                        status_lines: Self::repo_status_lines(Path::new(&repo.path)),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let artifact = ScopeBaselineArtifact {
            attempt_id,
            repo_snapshots,
            tmp_entries: Self::list_tmp_entries(),
        };
        self.write_scope_baseline_artifact(&artifact)
    }

    fn verify_scope_environment_baseline(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        let Ok(baseline) = self.read_scope_baseline_artifact(attempt_id) else {
            return Vec::new();
        };
        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return Vec::new();
        };
        let allowed_roots: Vec<PathBuf> = worktrees
            .iter()
            .filter_map(|(_, status)| status.path.canonicalize().ok())
            .collect();

        let mut violations = Vec::new();
        for snapshot in &baseline.repo_snapshots {
            let repo_path = Path::new(&snapshot.repo_path);
            let current_head = Self::repo_git_head(repo_path);
            if current_head != snapshot.git_head {
                violations.push(crate::core::enforcement::ScopeViolation::git(format!(
                    "Repository HEAD changed outside task worktree (before: {:?}, after: {:?})",
                    snapshot.git_head, current_head
                )));
            }

            let current_status = Self::repo_status_lines(repo_path);
            if current_status != snapshot.status_lines {
                let path_preview = current_status
                    .first()
                    .and_then(|line| line.strip_prefix("?? ").or_else(|| line.get(3..)))
                    .map_or("<unknown>", str::trim);
                violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                    format!("{}:{path_preview}", snapshot.repo_path),
                    "Repository workspace changed outside task worktree",
                ));
            }
        }

        let current_tmp = Self::list_tmp_entries();
        let baseline_tmp: HashSet<String> = baseline.tmp_entries.into_iter().collect();
        for created in current_tmp
            .into_iter()
            .filter(|name| !baseline_tmp.contains(name))
            .take(32)
        {
            let path = PathBuf::from("/tmp").join(&created);
            let canonical = path.canonicalize().unwrap_or(path);
            if allowed_roots.iter().any(|root| canonical.starts_with(root)) {
                continue;
            }
            if Self::scope_trace_is_ignored(&canonical, None, &self.config.data_dir) {
                continue;
            }
            violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                canonical.to_string_lossy().to_string(),
                "Filesystem write outside task worktree detected in /tmp",
            ));
        }

        violations
    }

    fn parse_scope_trace_written_paths(trace_contents: &str) -> Vec<PathBuf> {
        fn first_quoted_segment(line: &str) -> Option<String> {
            let start = line.find('"')?;
            let mut escaped = false;
            let mut out = String::new();
            for ch in line[start + 1..].chars() {
                if escaped {
                    out.push(ch);
                    escaped = false;
                    continue;
                }
                match ch {
                    '\\' => escaped = true,
                    '"' => return Some(out),
                    _ => out.push(ch),
                }
            }
            None
        }

        let mut paths = Vec::new();
        for line in trace_contents.lines() {
            let is_open = line.contains("open(") || line.contains("openat(");
            if !is_open {
                continue;
            }
            let has_write_intent = line.contains("O_WRONLY")
                || line.contains("O_RDWR")
                || line.contains("O_CREAT")
                || line.contains("O_TRUNC")
                || line.contains("O_APPEND");
            if !has_write_intent {
                continue;
            }
            let Some(path) = first_quoted_segment(line).filter(|p| !p.is_empty()) else {
                continue;
            };
            paths.push(PathBuf::from(path));
        }

        paths.sort();
        paths.dedup();
        paths
    }

    fn scope_trace_is_ignored(path: &Path, home: Option<&Path>, data_dir: &Path) -> bool {
        let ignored_roots = [
            Path::new("/dev"),
            Path::new("/proc"),
            Path::new("/sys"),
            Path::new("/run"),
        ];
        if ignored_roots.iter().any(|root| path.starts_with(root)) {
            return true;
        }
        if path.starts_with(data_dir) {
            return true;
        }
        if let Some(home_dir) = home {
            if path.starts_with(home_dir.join(".config"))
                || path.starts_with(home_dir.join(".cache"))
                || path.starts_with(home_dir.join(".local/share"))
                || path.starts_with(home_dir.join(".npm"))
                || path.starts_with(home_dir.join(".bun"))
            {
                return true;
            }
        }
        false
    }

    fn verify_scope_trace_writes(
        &self,
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Vec<crate::core::enforcement::ScopeViolation> {
        let trace_path = self.scope_trace_path(attempt_id);
        let Ok(contents) = fs::read_to_string(&trace_path) else {
            return Vec::new();
        };

        let Ok(worktrees) = Self::inspect_task_worktrees(flow, state, task_id, origin) else {
            return Vec::new();
        };
        let allowed_roots: Vec<PathBuf> = worktrees
            .iter()
            .filter_map(|(_, status)| status.path.canonicalize().ok())
            .collect();
        let home_dir = env::var("HOME").ok().map(PathBuf::from);

        let mut violations = Vec::new();
        for observed in Self::parse_scope_trace_written_paths(&contents) {
            let observed_abs = if observed.is_absolute() {
                observed
            } else if let Some((_, first)) = worktrees.first() {
                first.path.join(observed)
            } else {
                continue;
            };

            let canonical = observed_abs
                .canonicalize()
                .unwrap_or_else(|_| observed_abs.clone());
            if Self::scope_trace_is_ignored(&canonical, home_dir.as_deref(), &self.config.data_dir)
            {
                continue;
            }
            if allowed_roots.iter().any(|root| canonical.starts_with(root)) {
                continue;
            }

            violations.push(crate::core::enforcement::ScopeViolation::filesystem(
                canonical.to_string_lossy().to_string(),
                "Write outside task worktree detected via runtime syscall trace",
            ));
        }

        violations.sort_by(|a, b| a.path.cmp(&b.path));
        violations.dedup_by(|a, b| a.path == b.path);
        violations
    }

    fn emit_task_execution_completion_events(
        &self,
        flow: &TaskFlow,
        task_id: Uuid,
        attempt: &AttemptState,
        completion: CompletionArtifacts<'_>,
        origin: &'static str,
    ) -> Result<()> {
        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, task_id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt.id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: Some(attempt.id),
                    from: TaskExecState::Running,
                    to: TaskExecState::Verifying,
                },
                corr_task,
            ),
            origin,
        )?;

        if let Some(commit_sha) = completion.checkpoint_commit_sha {
            self.append_event(
                Event::new(
                    EventPayload::CheckpointCommitCreated {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        commit_sha,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        for change in &completion.artifact.diff.changes {
            self.append_event(
                Event::new(
                    EventPayload::FileModified {
                        flow_id: flow.id,
                        task_id,
                        attempt_id: attempt.id,
                        path: change.path.to_string_lossy().to_string(),
                        change_type: change.change_type,
                        old_hash: change.old_hash.clone(),
                        new_hash: change.new_hash.clone(),
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::DiffComputed {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: attempt.id,
                    diff_id: completion.artifact.diff.id,
                    baseline_id: completion.baseline_id,
                    change_count: completion.artifact.diff.change_count(),
                },
                corr_attempt,
            ),
            origin,
        )?;

        Ok(())
    }

    fn capture_and_store_baseline(
        &self,
        worktree_path: &Path,
        origin: &'static str,
    ) -> Result<Baseline> {
        let baseline = Baseline::capture(worktree_path)
            .map_err(|e| HivemindError::system("baseline_capture_failed", e.to_string(), origin))?;
        self.write_baseline_artifact(&baseline)?;
        Ok(baseline)
    }

    fn compute_and_store_diff(
        &self,
        baseline_id: Uuid,
        worktree_path: &Path,
        task_id: Uuid,
        attempt_id: Uuid,
        origin: &'static str,
    ) -> Result<DiffArtifact> {
        let baseline = self.read_baseline_artifact(baseline_id)?;
        let diff = Diff::compute(&baseline, worktree_path)
            .map_err(|e| HivemindError::system("diff_compute_failed", e.to_string(), origin))?
            .for_task(task_id)
            .for_attempt(attempt_id);

        let mut unified = String::new();
        for change in &diff.changes {
            if let Ok(chunk) = self.unified_diff_for_change(baseline_id, worktree_path, change) {
                unified.push_str(&chunk);
                if !chunk.ends_with('\n') {
                    unified.push('\n');
                }
            }
        }

        let artifact = DiffArtifact { diff, unified };
        self.write_diff_artifact(&artifact)?;
        Ok(artifact)
    }

    fn artifacts_dir(&self) -> PathBuf {
        self.config.data_dir.join("artifacts")
    }

    fn baselines_dir(&self) -> PathBuf {
        self.artifacts_dir().join("baselines")
    }

    fn baseline_dir(&self, baseline_id: Uuid) -> PathBuf {
        self.baselines_dir().join(baseline_id.to_string())
    }

    fn baseline_json_path(&self, baseline_id: Uuid) -> PathBuf {
        self.baseline_dir(baseline_id).join("baseline.json")
    }

    fn baseline_files_dir(&self, baseline_id: Uuid) -> PathBuf {
        self.baseline_dir(baseline_id).join("files")
    }

    fn diffs_dir(&self) -> PathBuf {
        self.artifacts_dir().join("diffs")
    }

    fn diff_json_path(&self, diff_id: Uuid) -> PathBuf {
        self.diffs_dir().join(format!("{diff_id}.json"))
    }

    fn scope_traces_dir(&self) -> PathBuf {
        self.artifacts_dir().join("scope-traces")
    }

    fn scope_trace_path(&self, attempt_id: Uuid) -> PathBuf {
        self.scope_traces_dir().join(format!("{attempt_id}.log"))
    }

    fn scope_baselines_dir(&self) -> PathBuf {
        self.artifacts_dir().join("scope-baselines")
    }

    fn scope_baseline_path(&self, attempt_id: Uuid) -> PathBuf {
        self.scope_baselines_dir()
            .join(format!("{attempt_id}.json"))
    }

    fn governance_projects_dir(&self) -> PathBuf {
        self.config.data_dir.join("projects")
    }

    fn governance_project_root(&self, project_id: Uuid) -> PathBuf {
        self.governance_projects_dir().join(project_id.to_string())
    }

    fn governance_global_root(&self) -> PathBuf {
        self.config.data_dir.join("global")
    }

    fn governance_artifact_locations(&self, project_id: Uuid) -> Vec<GovernanceArtifactLocation> {
        let project_root = self.governance_project_root(project_id);
        let global_root = self.governance_global_root();
        vec![
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "constitution",
                artifact_key: "constitution.yaml".to_string(),
                is_dir: false,
                path: project_root.join("constitution.yaml"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "documents",
                artifact_key: "documents".to_string(),
                is_dir: true,
                path: project_root.join("documents"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: project_root.join("notepad.md"),
            },
            GovernanceArtifactLocation {
                project_id: Some(project_id),
                scope: "project",
                artifact_kind: "graph_snapshot",
                artifact_key: "graph_snapshot.json".to_string(),
                is_dir: false,
                path: project_root.join("graph_snapshot.json"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "skills",
                artifact_key: "skills".to_string(),
                is_dir: true,
                path: global_root.join("skills"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "system_prompts",
                artifact_key: "system_prompts".to_string(),
                is_dir: true,
                path: global_root.join("system_prompts"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "templates",
                artifact_key: "templates".to_string(),
                is_dir: true,
                path: global_root.join("templates"),
            },
            GovernanceArtifactLocation {
                project_id: None,
                scope: "global",
                artifact_kind: "notepad",
                artifact_key: "notepad.md".to_string(),
                is_dir: false,
                path: global_root.join("notepad.md"),
            },
        ]
    }

    fn governance_default_file_contents(location: &GovernanceArtifactLocation) -> &'static [u8] {
        match (location.scope, location.artifact_kind) {
            ("project", "constitution") => {
                b"# Hivemind project constitution\nversion: 1\nrules: []\n"
            }
            ("project", "graph_snapshot") => b"{}\n",
            _ => b"\n",
        }
    }

    fn governance_artifact_projection_key(location: &GovernanceArtifactLocation) -> String {
        format!(
            "{}::{}::{}::{}",
            location
                .project_id
                .map_or_else(|| "global".to_string(), |id| id.to_string()),
            location.scope,
            location.artifact_kind,
            location.artifact_key
        )
    }

    fn governance_projection_for_location<'a>(
        state: &'a AppState,
        location: &GovernanceArtifactLocation,
    ) -> Option<&'a crate::core::state::GovernanceArtifact> {
        state
            .governance_artifacts
            .get(&Self::governance_artifact_projection_key(location))
    }

    fn next_governance_revision(
        state: &AppState,
        location: &GovernanceArtifactLocation,
        pending_revisions: &mut HashMap<String, u64>,
    ) -> u64 {
        let key = Self::governance_artifact_projection_key(location);
        if let Some(next) = pending_revisions.get_mut(&key) {
            *next = next.saturating_add(1);
            return *next;
        }
        let next = Self::governance_projection_for_location(state, location)
            .map_or(1, |existing| existing.revision.saturating_add(1));
        pending_revisions.insert(key, next);
        next
    }

    fn ensure_governance_layout(
        &self,
        project_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<PathBuf>> {
        let mut created = Vec::new();
        for location in self.governance_artifact_locations(project_id) {
            if location.is_dir {
                if location.path.exists() {
                    if !location.path.is_dir() {
                        return Err(HivemindError::system(
                            "governance_path_conflict",
                            format!(
                                "Expected governance directory at '{}' but found a file",
                                location.path.display()
                            ),
                            origin,
                        ));
                    }
                } else {
                    fs::create_dir_all(&location.path).map_err(|e| {
                        HivemindError::system(
                            "governance_storage_create_failed",
                            e.to_string(),
                            origin,
                        )
                    })?;
                    created.push(location.path.clone());
                }
                continue;
            }

            if let Some(parent) = location.path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
                })?;
            }

            if location.path.exists() {
                if location.path.is_dir() {
                    return Err(HivemindError::system(
                        "governance_path_conflict",
                        format!(
                            "Expected governance file at '{}' but found a directory",
                            location.path.display()
                        ),
                        origin,
                    ));
                }
            } else {
                fs::write(
                    &location.path,
                    Self::governance_default_file_contents(&location),
                )
                .map_err(|e| {
                    HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
                })?;
                created.push(location.path.clone());
            }
        }
        Ok(created)
    }

    fn legacy_governance_mappings(
        &self,
        project: &Project,
    ) -> Vec<LegacyGovernanceArtifactMapping> {
        let mut mappings = Vec::new();
        let canonical = self.governance_artifact_locations(project.id);
        for repo in &project.repositories {
            let legacy_root = PathBuf::from(&repo.path).join(".hivemind");
            for destination in &canonical {
                let source = if destination.scope == "project" {
                    legacy_root.join(&destination.artifact_key)
                } else if destination.artifact_key == "notepad.md" {
                    legacy_root.join("global").join("notepad.md")
                } else {
                    legacy_root.join("global").join(&destination.artifact_key)
                };
                mappings.push(LegacyGovernanceArtifactMapping {
                    source,
                    destination: destination.clone(),
                });
            }
        }
        mappings
    }

    fn copy_dir_if_missing(
        source: &Path,
        destination: &Path,
        origin: &'static str,
    ) -> Result<bool> {
        if !source.exists() || !source.is_dir() {
            return Ok(false);
        }

        fs::create_dir_all(destination).map_err(|e| {
            HivemindError::system("governance_migration_failed", e.to_string(), origin)
        })?;

        let mut copied = false;
        let mut stack = vec![(source.to_path_buf(), destination.to_path_buf())];
        while let Some((src_dir, dst_dir)) = stack.pop() {
            for entry in fs::read_dir(&src_dir).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })? {
                let entry = entry.map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                let src_path = entry.path();
                let dst_path = dst_dir.join(entry.file_name());
                let file_type = entry.file_type().map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                if file_type.is_dir() {
                    fs::create_dir_all(&dst_path).map_err(|e| {
                        HivemindError::system("governance_migration_failed", e.to_string(), origin)
                    })?;
                    stack.push((src_path, dst_path));
                    continue;
                }

                if dst_path.exists() {
                    continue;
                }
                fs::copy(&src_path, &dst_path).map_err(|e| {
                    HivemindError::system("governance_migration_failed", e.to_string(), origin)
                })?;
                copied = true;
            }
        }

        Ok(copied)
    }

    fn copy_file_if_missing(
        source: &Path,
        destination: &Path,
        default_contents: Option<&[u8]>,
        origin: &'static str,
    ) -> Result<bool> {
        if !source.exists() || !source.is_file() {
            return Ok(false);
        }
        if destination.exists() {
            let source_bytes = fs::read(source).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            if source_bytes.is_empty() {
                return Ok(false);
            }

            let destination_bytes = fs::read(destination).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            if destination_bytes == source_bytes {
                return Ok(false);
            }

            let can_overwrite_scaffold = default_contents
                .is_some_and(|default_bytes| destination_bytes.as_slice() == default_bytes);
            if !destination_bytes.is_empty() && !can_overwrite_scaffold {
                return Ok(false);
            }

            fs::write(destination, source_bytes).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
            return Ok(true);
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HivemindError::system("governance_migration_failed", e.to_string(), origin)
            })?;
        }
        fs::copy(source, destination).map_err(|e| {
            HivemindError::system("governance_migration_failed", e.to_string(), origin)
        })?;
        Ok(true)
    }

    fn write_baseline_artifact(&self, baseline: &Baseline) -> Result<()> {
        let files_dir = self.baseline_files_dir(baseline.id);
        fs::create_dir_all(&files_dir).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;

        let json = serde_json::to_vec_pretty(baseline).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;
        fs::write(self.baseline_json_path(baseline.id), json).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_baseline_artifact",
            )
        })?;

        for snapshot in baseline.files.values() {
            if snapshot.is_dir {
                continue;
            }

            let src = baseline.root.join(&snapshot.path);
            let dst = files_dir.join(&snapshot.path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "artifact_write_failed",
                        e.to_string(),
                        "registry:write_baseline_artifact",
                    )
                })?;
            }

            let Ok(contents) = fs::read(src) else {
                continue;
            };
            let _ = fs::write(dst, contents);
        }
        Ok(())
    }

    fn read_baseline_artifact(&self, baseline_id: Uuid) -> Result<Baseline> {
        let bytes = fs::read(self.baseline_json_path(baseline_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_baseline_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_baseline_artifact",
            )
        })
    }

    fn write_diff_artifact(&self, artifact: &DiffArtifact) -> Result<()> {
        fs::create_dir_all(self.diffs_dir()).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        let json = serde_json::to_vec_pretty(artifact).map_err(|e| {
            HivemindError::system(
                "artifact_serialize_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        fs::write(self.diff_json_path(artifact.diff.id), json).map_err(|e| {
            HivemindError::system(
                "artifact_write_failed",
                e.to_string(),
                "registry:write_diff_artifact",
            )
        })?;
        Ok(())
    }

    fn read_diff_artifact(&self, diff_id: Uuid) -> Result<DiffArtifact> {
        let bytes = fs::read(self.diff_json_path(diff_id)).map_err(|e| {
            HivemindError::system(
                "artifact_read_failed",
                e.to_string(),
                "registry:read_diff_artifact",
            )
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            HivemindError::system(
                "artifact_deserialize_failed",
                e.to_string(),
                "registry:read_diff_artifact",
            )
        })
    }

    fn unified_diff_for_change(
        &self,
        baseline_id: Uuid,
        worktree_root: &std::path::Path,
        change: &FileChange,
    ) -> std::io::Result<String> {
        let baseline_files = self.baseline_files_dir(baseline_id);
        let old = baseline_files.join(&change.path);
        let new = worktree_root.join(&change.path);

        match change.change_type {
            ChangeType::Created => unified_diff(None, Some(&new)),
            ChangeType::Deleted => unified_diff(Some(&old), None),
            ChangeType::Modified => unified_diff(Some(&old), Some(&new)),
        }
    }

    fn worktree_error_to_hivemind(err: WorktreeError, origin: &'static str) -> HivemindError {
        match err {
            WorktreeError::InvalidRepo(path) => HivemindError::git(
                "invalid_repo",
                format!("Invalid git repository: {}", path.display()),
                origin,
            ),
            WorktreeError::GitError(msg) => HivemindError::git("git_worktree_failed", msg, origin),
            WorktreeError::AlreadyExists(task_id) => HivemindError::user(
                "worktree_already_exists",
                format!("Worktree already exists for task {task_id}"),
                origin,
            ),
            WorktreeError::NotFound(id) => HivemindError::user(
                "worktree_not_found",
                format!("Worktree not found: {id}"),
                origin,
            ),
            WorktreeError::IoError(e) => {
                HivemindError::system("worktree_io_error", e.to_string(), origin)
            }
        }
    }

    fn project_for_flow<'a>(flow: &TaskFlow, state: &'a AppState) -> Result<&'a Project> {
        state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:worktree_manager_for_flow",
            )
        })
    }

    fn worktree_managers_for_flow(
        flow: &TaskFlow,
        state: &AppState,
        origin: &'static str,
    ) -> Result<Vec<(String, WorktreeManager)>> {
        let project = Self::project_for_flow(flow, state)?;

        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no repository attached",
                origin,
            )
            .with_hint("Attach a repo via 'hivemind project attach-repo <project> <path>'"));
        }

        project
            .repositories
            .iter()
            .map(|repo| {
                WorktreeManager::new(PathBuf::from(&repo.path), WorktreeConfig::default())
                    .map(|manager| (repo.name.clone(), manager))
                    .map_err(|e| Self::worktree_error_to_hivemind(e, origin))
            })
            .collect()
    }

    fn worktree_manager_for_flow(flow: &TaskFlow, state: &AppState) -> Result<WorktreeManager> {
        let managers =
            Self::worktree_managers_for_flow(flow, state, "registry:worktree_manager_for_flow")?;
        managers
            .into_iter()
            .next()
            .map(|(_, manager)| manager)
            .ok_or_else(|| {
                HivemindError::user(
                    "project_has_no_repo",
                    "Project has no repository attached",
                    "registry:worktree_manager_for_flow",
                )
            })
    }

    fn git_ref_exists(repo_path: &Path, reference: &str) -> bool {
        std::process::Command::new("git")
            .current_dir(repo_path)
            .args(["show-ref", "--verify", "--quiet", reference])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn default_base_ref_for_repo(
        flow: &TaskFlow,
        manager: &WorktreeManager,
        is_primary: bool,
    ) -> String {
        let flow_ref = format!("refs/heads/flow/{}", flow.id);
        if Self::git_ref_exists(manager.repo_path(), &flow_ref) {
            return format!("flow/{}", flow.id);
        }
        if is_primary {
            return flow
                .base_revision
                .clone()
                .unwrap_or_else(|| "HEAD".to_string());
        }
        "HEAD".to_string()
    }

    fn ensure_task_worktree_status(
        manager: &WorktreeManager,
        flow: &TaskFlow,
        task_id: Uuid,
        base_ref: &str,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if status.is_worktree {
            return Ok(status);
        }

        manager
            .create(flow.id, task_id, Some(base_ref))
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if !status.is_worktree {
            return Err(HivemindError::git(
                "worktree_create_failed",
                format!(
                    "Worktree path exists but is not a git worktree: {}",
                    status.path.display()
                ),
                origin,
            ));
        }
        Ok(status)
    }

    fn ensure_task_worktree(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let managers = Self::worktree_managers_for_flow(flow, state, origin)?;
        let mut primary_status: Option<WorktreeStatus> = None;
        for (idx, (_repo_name, manager)) in managers.iter().enumerate() {
            let base_ref = Self::default_base_ref_for_repo(flow, manager, idx == 0);
            let status =
                Self::ensure_task_worktree_status(manager, flow, task_id, &base_ref, origin)?;
            if idx == 0 {
                primary_status = Some(status);
            }
        }
        primary_status.ok_or_else(|| {
            HivemindError::user(
                "project_has_no_repo",
                "Project has no repository attached",
                origin,
            )
        })
    }

    fn inspect_task_worktrees(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<Vec<(String, WorktreeStatus)>> {
        let managers = Self::worktree_managers_for_flow(flow, state, origin)?;
        let mut statuses = Vec::new();
        for (repo_name, manager) in managers {
            let status = manager
                .inspect(flow.id, task_id)
                .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
            if !status.is_worktree {
                return Err(HivemindError::user(
                    "worktree_not_found",
                    format!("Worktree not found for task in repo '{repo_name}'"),
                    origin,
                ));
            }
            statuses.push((repo_name, status));
        }
        Ok(statuses)
    }

    /// Opens or creates a registry at the default location.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open() -> Result<Self> {
        Self::open_with_config(RegistryConfig::default_dir())
    }

    /// Opens or creates a registry with custom config.
    ///
    /// # Errors
    /// Returns an error if the event store cannot be opened.
    pub fn open_with_config(config: RegistryConfig) -> Result<Self> {
        let store = IndexedEventStore::open(&config.data_dir).map_err(|e| {
            HivemindError::system("store_open_failed", e.to_string(), "registry:open")
        })?;

        Ok(Self {
            store: Arc::new(store),
            config,
        })
    }

    /// Creates a registry with a custom event store (for testing).
    #[must_use]
    pub fn with_store(store: Arc<dyn EventStore>, config: RegistryConfig) -> Self {
        Self { store, config }
    }

    /// Gets the current state by replaying all events.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn state(&self) -> Result<AppState> {
        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("state_read_failed", e.to_string(), "registry:state")
        })?;
        Ok(AppState::replay(&events))
    }

    /// Lists events in the store.
    ///
    /// # Errors
    /// Returns an error if events cannot be read.
    pub fn list_events(&self, project_id: Option<Uuid>, limit: usize) -> Result<Vec<Event>> {
        let mut filter = EventFilter::all();
        filter.project_id = project_id;
        filter.limit = Some(limit);

        self.store.read(&filter).map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:list_events")
        })
    }

    pub fn read_events(&self, filter: &EventFilter) -> Result<Vec<Event>> {
        self.store.read(filter).map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:read_events")
        })
    }

    pub fn stream_events(&self, filter: &EventFilter) -> Result<std::sync::mpsc::Receiver<Event>> {
        self.store.stream(filter).map_err(|e| {
            HivemindError::system(
                "event_stream_failed",
                e.to_string(),
                "registry:stream_events",
            )
        })
    }

    /// Gets a specific event by ID.
    ///
    /// # Errors
    /// Returns an error if the event cannot be read or is not found.
    pub fn get_event(&self, event_id: &str) -> Result<Event> {
        let id = Uuid::parse_str(event_id).map_err(|_| {
            HivemindError::user(
                "invalid_event_id",
                format!("'{event_id}' is not a valid event ID"),
                "registry:get_event",
            )
        })?;

        let events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:get_event")
        })?;

        events
            .into_iter()
            .find(|e| e.metadata.id.as_uuid() == id)
            .ok_or_else(|| {
                HivemindError::user(
                    "event_not_found",
                    format!("Event '{event_id}' not found"),
                    "registry:get_event",
                )
            })
    }

    /// Creates a new project.
    ///
    /// # Errors
    /// Returns an error if a project with that name already exists.
    pub fn create_project(&self, name: &str, description: Option<&str>) -> Result<Project> {
        if name.trim().is_empty() {
            let err = HivemindError::user(
                "invalid_project_name",
                "Project name cannot be empty",
                "registry:create_project",
            )
            .with_hint("Provide a non-empty project name");
            self.record_error_event(&err, CorrelationIds::none());
            return Err(err);
        }

        let state = self.state()?;

        // Check for duplicate name
        if state.projects.values().any(|p| p.name == name) {
            let err = HivemindError::user(
                "project_exists",
                format!("Project '{name}' already exists"),
                "registry:create_project",
            )
            .with_hint("Choose a different project name");
            self.record_error_event(&err, CorrelationIds::none());
            return Err(err);
        }

        let id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::ProjectCreated {
                id,
                name: name.to_string(),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:create_project",
            )
        })?;

        // Return the created project by replaying
        let new_state = self.state()?;
        new_state.projects.get(&id).cloned().ok_or_else(|| {
            HivemindError::system(
                "project_not_found_after_create",
                "Project was not found after creation",
                "registry:create_project",
            )
        })
    }

    /// Lists all projects.
    ///
    /// # Errors
    /// Returns an error if state cannot be read.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let state = self.state()?;
        let mut projects: Vec<_> = state.projects.into_values().collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

    /// Gets a project by ID or name.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn get_project(&self, id_or_name: &str) -> Result<Project> {
        let state = self.state()?;

        // Try parsing as UUID first
        if let Ok(id) = Uuid::parse_str(id_or_name) {
            if let Some(project) = state.projects.get(&id) {
                return Ok(project.clone());
            }
        }

        // Search by name
        state
            .projects
            .values()
            .find(|p| p.name == id_or_name)
            .cloned()
            .ok_or_else(|| {
                HivemindError::user(
                    "project_not_found",
                    format!("Project '{id_or_name}' not found"),
                    "registry:get_project",
                )
                .with_hint("Use 'hivemind project list' to see available projects")
            })
    }

    /// Updates a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn update_project(
        &self,
        id_or_name: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if let Some(new_name) = name {
            if new_name.trim().is_empty() {
                let err = HivemindError::user(
                    "invalid_project_name",
                    "Project name cannot be empty",
                    "registry:update_project",
                )
                .with_hint("Provide a non-empty project name");
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        }

        let name = name.filter(|n| *n != project.name);
        let description = description.filter(|d| project.description.as_deref() != Some(*d));

        if name.is_none() && description.is_none() {
            return Ok(project);
        }

        // Check for name conflict if changing name
        if let Some(new_name) = name {
            let state = self.state()?;
            if state
                .projects
                .values()
                .any(|p| p.name == new_name && p.id != project.id)
            {
                let err = HivemindError::user(
                    "project_name_conflict",
                    format!("Project name '{new_name}' is already taken"),
                    "registry:update_project",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        }

        let event = Event::new(
            EventPayload::ProjectUpdated {
                id: project.id,
                name: name.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:update_project",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set(
        &self,
        id_or_name: &str,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        self.project_runtime_set_role(
            id_or_name,
            RuntimeRole::Worker,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
            max_parallel_tasks,
        )
    }

    fn ensure_supported_runtime_adapter(adapter: &str, origin: &'static str) -> Result<()> {
        if !SUPPORTED_ADAPTERS.contains(&adapter) {
            return Err(HivemindError::user(
                "invalid_runtime_adapter",
                format!(
                    "Unsupported runtime adapter '{adapter}'. Supported: {}",
                    SUPPORTED_ADAPTERS.join(", ")
                ),
                origin,
            ));
        }
        Ok(())
    }

    fn parse_runtime_env_pairs(
        env: &[String],
        origin: &'static str,
    ) -> Result<HashMap<String, String>> {
        let mut env_map = HashMap::new();
        for pair in env {
            let Some((k, v)) = pair.split_once('=') else {
                return Err(HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. Expected KEY=VALUE"),
                    origin,
                ));
            };
            if k.trim().is_empty() {
                return Err(HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. KEY cannot be empty"),
                    origin,
                ));
            }
            env_map.insert(k.to_string(), v.to_string());
        }
        Ok(env_map)
    }

    fn project_runtime_for_role(
        project: &Project,
        role: RuntimeRole,
    ) -> Option<ProjectRuntimeConfig> {
        match role {
            RuntimeRole::Worker => project
                .runtime_defaults
                .worker
                .clone()
                .or_else(|| project.runtime.clone()),
            RuntimeRole::Validator => project.runtime_defaults.validator.clone(),
        }
    }

    fn task_runtime_override_for_role(task: &Task, role: RuntimeRole) -> Option<TaskRuntimeConfig> {
        match role {
            RuntimeRole::Worker => task
                .runtime_overrides
                .worker
                .clone()
                .or_else(|| task.runtime_override.clone()),
            RuntimeRole::Validator => task.runtime_overrides.validator.clone(),
        }
    }

    fn task_runtime_to_project_runtime(
        runtime: TaskRuntimeConfig,
        max_parallel_tasks: u16,
    ) -> ProjectRuntimeConfig {
        ProjectRuntimeConfig {
            adapter_name: runtime.adapter_name,
            binary_path: runtime.binary_path,
            model: runtime.model,
            args: runtime.args,
            env: runtime.env,
            timeout_ms: runtime.timeout_ms,
            max_parallel_tasks,
        }
    }

    fn max_parallel_from_defaults(defaults: &RuntimeRoleDefaults) -> Option<u16> {
        defaults.worker.as_ref().map(|cfg| cfg.max_parallel_tasks)
    }

    fn effective_runtime_for_task(
        state: &AppState,
        flow: &TaskFlow,
        task_id: Uuid,
        role: RuntimeRole,
        origin: &'static str,
    ) -> Result<ProjectRuntimeConfig> {
        let task = state.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_not_found",
                format!("Task '{task_id}' not found"),
                origin,
            )
        })?;

        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                origin,
            )
        })?;

        let flow_defaults = state
            .flow_runtime_defaults
            .get(&flow.id)
            .cloned()
            .unwrap_or_default();

        let project_defaults = project.runtime_defaults.clone();
        let global_defaults = state.global_runtime_defaults.clone();

        let max_parallel = match role {
            RuntimeRole::Worker => Self::max_parallel_from_defaults(&flow_defaults)
                .or_else(|| Self::max_parallel_from_defaults(&project_defaults))
                .or_else(|| project.runtime.as_ref().map(|cfg| cfg.max_parallel_tasks))
                .or_else(|| Self::max_parallel_from_defaults(&global_defaults))
                .unwrap_or(1),
            RuntimeRole::Validator => Self::max_parallel_from_defaults(&flow_defaults)
                .or_else(|| Self::max_parallel_from_defaults(&project_defaults))
                .or_else(|| project.runtime.as_ref().map(|cfg| cfg.max_parallel_tasks))
                .or_else(|| Self::max_parallel_from_defaults(&global_defaults))
                .unwrap_or(1),
        };

        if let Some(task_rt) = Self::task_runtime_override_for_role(task, role) {
            return Ok(Self::task_runtime_to_project_runtime(task_rt, max_parallel));
        }
        if let Some(flow_rt) = match role {
            RuntimeRole::Worker => flow_defaults.worker,
            RuntimeRole::Validator => flow_defaults.validator,
        } {
            return Ok(flow_rt);
        }
        if let Some(project_rt) = Self::project_runtime_for_role(project, role) {
            return Ok(project_rt);
        }
        if let Some(global_rt) = match role {
            RuntimeRole::Worker => global_defaults.worker,
            RuntimeRole::Validator => global_defaults.validator,
        } {
            return Ok(global_rt);
        }

        Err(HivemindError::new(
            ErrorCategory::Runtime,
            "runtime_not_configured",
            format!("No runtime configured for role '{role:?}'"),
            origin,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn project_runtime_set_role(
        &self,
        id_or_name: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if max_parallel_tasks == 0 {
            let err = HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:project_runtime_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if let Err(err) =
            Self::ensure_supported_runtime_adapter(adapter, "registry:project_runtime_set")
        {
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let env_map = match Self::parse_runtime_env_pairs(env, "registry:project_runtime_set") {
            Ok(parsed) => parsed,
            Err(err) => {
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
        };

        let desired = crate::core::state::ProjectRuntimeConfig {
            adapter_name: adapter.to_string(),
            binary_path: binary_path.to_string(),
            model: model.clone(),
            args: args.to_vec(),
            env: env_map.clone(),
            timeout_ms,
            max_parallel_tasks,
        };
        let current = Self::project_runtime_for_role(&project, role);
        if current.as_ref() == Some(&desired) {
            return Ok(project);
        }

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::ProjectRuntimeConfigured {
                    project_id: project.id,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::ProjectRuntimeRoleConfigured {
                    project_id: project.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_project(project.id),
            ),
        };

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:project_runtime_set",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn runtime_defaults_set(
        &self,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<()> {
        if max_parallel_tasks == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:runtime_defaults_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher"));
        }
        Self::ensure_supported_runtime_adapter(adapter, "registry:runtime_defaults_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:runtime_defaults_set")?;

        self.append_event(
            Event::new(
                EventPayload::GlobalRuntimeConfigured {
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::none(),
            ),
            "registry:runtime_defaults_set",
        )
    }

    #[must_use]
    pub fn runtime_list(&self) -> Vec<RuntimeListEntry> {
        runtime_descriptors()
            .into_iter()
            .map(|d| RuntimeListEntry {
                adapter_name: d.adapter_name.to_string(),
                default_binary: d.default_binary.to_string(),
                available: Self::binary_available(d.default_binary),
                opencode_compatible: d.opencode_compatible,
            })
            .collect()
    }

    pub fn runtime_health(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<RuntimeHealthStatus> {
        self.runtime_health_with_role(project, task_id, None, RuntimeRole::Worker)
    }

    #[allow(clippy::too_many_lines)]
    pub fn runtime_health_with_role(
        &self,
        project: Option<&str>,
        task_id: Option<&str>,
        flow_id: Option<&str>,
        role: RuntimeRole,
    ) -> Result<RuntimeHealthStatus> {
        if let Some(flow_id) = flow_id {
            let flow = self.get_flow(flow_id)?;
            let state = self.state()?;
            let flow_defaults = state
                .flow_runtime_defaults
                .get(&flow.id)
                .cloned()
                .unwrap_or_default();
            let runtime = match role {
                RuntimeRole::Worker => flow_defaults.worker,
                RuntimeRole::Validator => flow_defaults.validator,
            }
            .or_else(|| {
                state
                    .projects
                    .get(&flow.project_id)
                    .and_then(|project| Self::project_runtime_for_role(project, role))
            })
            .or(match role {
                RuntimeRole::Worker => state.global_runtime_defaults.worker,
                RuntimeRole::Validator => state.global_runtime_defaults.validator,
            })
            .ok_or_else(|| {
                HivemindError::new(
                    ErrorCategory::Runtime,
                    "runtime_not_configured",
                    "Flow has no effective runtime configured for this role",
                    "registry:runtime_health",
                )
            })?;
            return Ok(Self::health_for_runtime(
                &runtime,
                Some(format!("flow:{flow_id}:{role:?}")),
            ));
        }

        if let Some(task_id) = task_id {
            let task_uuid = Uuid::parse_str(task_id).map_err(|_| {
                HivemindError::user(
                    "invalid_task_id",
                    format!("'{task_id}' is not a valid task ID"),
                    "registry:runtime_health",
                )
            })?;
            let state = self.state()?;
            let flow = state
                .flows
                .values()
                .filter(|f| f.task_executions.contains_key(&task_uuid))
                .max_by_key(|f| f.updated_at)
                .cloned()
                .ok_or_else(|| {
                    HivemindError::user(
                        "task_not_in_flow",
                        "Task is not part of any flow",
                        "registry:runtime_health",
                    )
                })?;
            let runtime = Self::effective_runtime_for_task(
                &state,
                &flow,
                task_uuid,
                role,
                "registry:runtime_health",
            )?;

            return Ok(Self::health_for_runtime(
                &runtime,
                Some(format!("task:{task_id}:{role:?}")),
            ));
        }

        if let Some(project_id_or_name) = project {
            let project = self.get_project(project_id_or_name)?;
            let runtime = Self::project_runtime_for_role(&project, role).ok_or_else(|| {
                HivemindError::new(
                    ErrorCategory::Runtime,
                    "runtime_not_configured",
                    "Project has no runtime configured",
                    "registry:runtime_health",
                )
            })?;
            return Ok(Self::health_for_runtime(
                &runtime,
                Some(format!("project:{project_id_or_name}:{role:?}")),
            ));
        }

        Ok(RuntimeHealthStatus {
            adapter_name: "all".to_string(),
            binary_path: "builtin-defaults".to_string(),
            healthy: self.runtime_list().iter().all(|r| r.available),
            target: None,
            details: Some(
                self.runtime_list()
                    .into_iter()
                    .map(|r| {
                        format!(
                            "{}={} ({})",
                            r.adapter_name,
                            if r.available { "ok" } else { "missing" },
                            r.default_binary
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn task_runtime_set(
        &self,
        task_id: &str,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
    ) -> Result<Task> {
        self.task_runtime_set_role(
            task_id,
            RuntimeRole::Worker,
            adapter,
            binary_path,
            model,
            args,
            env,
            timeout_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn task_runtime_set_role(
        &self,
        task_id: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
    ) -> Result<Task> {
        let task = self.get_task(task_id)?;
        Self::ensure_supported_runtime_adapter(adapter, "registry:task_runtime_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:task_runtime_set")?;

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::TaskRuntimeConfigured {
                    task_id: task.id,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::TaskRuntimeRoleConfigured {
                    task_id: task.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
        };
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:task_runtime_set",
            )
        })?;
        self.get_task(task_id)
    }

    pub fn task_runtime_clear(&self, task_id: &str) -> Result<Task> {
        self.task_runtime_clear_role(task_id, RuntimeRole::Worker)
    }

    pub fn task_runtime_clear_role(&self, task_id: &str, role: RuntimeRole) -> Result<Task> {
        let task = self.get_task(task_id)?;
        let already_cleared = match role {
            RuntimeRole::Worker => {
                task.runtime_override.is_none() && task.runtime_overrides.worker.is_none()
            }
            RuntimeRole::Validator => task.runtime_overrides.validator.is_none(),
        };
        if already_cleared {
            return Ok(task);
        }

        let event = match role {
            RuntimeRole::Worker => Event::new(
                EventPayload::TaskRuntimeCleared { task_id: task.id },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            RuntimeRole::Validator => Event::new(
                EventPayload::TaskRuntimeRoleCleared {
                    task_id: task.id,
                    role,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
        };
        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:task_runtime_clear",
            )
        })?;
        self.get_task(task_id)
    }

    pub fn task_set_run_mode(&self, task_id: &str, mode: RunMode) -> Result<Task> {
        let task = self.get_task(task_id)?;
        if task.run_mode == mode {
            return Ok(task);
        }

        self.append_event(
            Event::new(
                EventPayload::TaskRunModeSet {
                    task_id: task.id,
                    mode,
                },
                CorrelationIds::for_task(task.project_id, task.id),
            ),
            "registry:task_set_run_mode",
        )?;
        if mode == RunMode::Auto {
            let state = self.state()?;
            if let Some(flow) = state
                .flows
                .values()
                .filter(|flow| flow.task_executions.contains_key(&task.id))
                .max_by_key(|flow| flow.updated_at)
                .filter(|flow| flow.run_mode == RunMode::Auto && flow.state == FlowState::Running)
            {
                let _ = self.auto_progress_flow(&flow.id.to_string());
            }
        }
        self.get_task(task_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn flow_runtime_set(
        &self,
        flow_id: &str,
        role: RuntimeRole,
        adapter: &str,
        binary_path: &str,
        model: Option<String>,
        args: &[String],
        env: &[String],
        timeout_ms: u64,
        max_parallel_tasks: u16,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if max_parallel_tasks == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel_tasks",
                "max_parallel_tasks must be at least 1",
                "registry:flow_runtime_set",
            )
            .with_hint("Use --max-parallel-tasks 1 or higher"));
        }
        Self::ensure_supported_runtime_adapter(adapter, "registry:flow_runtime_set")?;
        let env_map = Self::parse_runtime_env_pairs(env, "registry:flow_runtime_set")?;
        self.append_event(
            Event::new(
                EventPayload::TaskFlowRuntimeConfigured {
                    flow_id: flow.id,
                    role,
                    adapter_name: adapter.to_string(),
                    binary_path: binary_path.to_string(),
                    model,
                    args: args.to_vec(),
                    env: env_map,
                    timeout_ms,
                    max_parallel_tasks,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:flow_runtime_set",
        )?;
        self.get_flow(flow_id)
    }

    pub fn flow_runtime_clear(&self, flow_id: &str, role: RuntimeRole) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        self.append_event(
            Event::new(
                EventPayload::TaskFlowRuntimeCleared {
                    flow_id: flow.id,
                    role,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:flow_runtime_clear",
        )?;
        self.get_flow(flow_id)
    }

    pub fn flow_set_run_mode(&self, flow_id: &str, mode: RunMode) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.run_mode == mode {
            return Ok(flow);
        }
        self.append_event(
            Event::new(
                EventPayload::TaskFlowRunModeSet {
                    flow_id: flow.id,
                    mode,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:flow_set_run_mode",
        )?;
        let updated = self.get_flow(flow_id)?;
        if mode == RunMode::Auto {
            if updated.state == FlowState::Created {
                let state = self.state()?;
                if Self::unmet_flow_dependencies(&state, &updated).is_empty() {
                    return self.start_flow(flow_id);
                }
            }
            if updated.state == FlowState::Running {
                return self.auto_progress_flow(flow_id);
            }
        }
        Ok(updated)
    }

    pub fn flow_add_dependency(&self, flow_id: &str, depends_on_flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        let dependency = self.get_flow(depends_on_flow_id)?;
        if flow.id == dependency.id {
            return Err(HivemindError::user(
                "flow_dependency_self",
                "A flow cannot depend on itself",
                "registry:flow_add_dependency",
            ));
        }
        if flow.project_id != dependency.project_id {
            return Err(HivemindError::user(
                "flow_dependency_cross_project",
                "Flows must belong to the same project",
                "registry:flow_add_dependency",
            ));
        }
        if flow.state != FlowState::Created {
            return Err(HivemindError::user(
                "flow_dependency_locked",
                "Flow dependencies can only be changed while flow is in CREATED state",
                "registry:flow_add_dependency",
            ));
        }

        let state = self.state()?;
        let mut deps = flow.depends_on_flows.clone();
        deps.insert(dependency.id);

        let has_cycle = |state: &AppState, start: Uuid, deps_snapshot: &HashSet<Uuid>| {
            let mut stack: Vec<Uuid> = deps_snapshot.iter().copied().collect();
            let mut seen: HashSet<Uuid> = HashSet::new();
            while let Some(next) = stack.pop() {
                if next == start {
                    return true;
                }
                if !seen.insert(next) {
                    continue;
                }
                if let Some(other) = state.flows.get(&next) {
                    for dep in &other.depends_on_flows {
                        stack.push(*dep);
                    }
                }
            }
            false
        };
        if has_cycle(&state, flow.id, &deps) {
            return Err(HivemindError::user(
                "flow_dependency_cycle",
                "Flow dependency introduces a cycle",
                "registry:flow_add_dependency",
            ));
        }

        self.append_event(
            Event::new(
                EventPayload::TaskFlowDependencyAdded {
                    flow_id: flow.id,
                    depends_on_flow_id: dependency.id,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:flow_add_dependency",
        )?;
        self.get_flow(flow_id)
    }

    fn binary_available(binary: &str) -> bool {
        if binary.contains('/') {
            let path = PathBuf::from(binary);
            return path.exists();
        }

        std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(binary);
                candidate.exists() && candidate.is_file()
            })
        })
    }

    fn health_for_runtime(
        runtime: &ProjectRuntimeConfig,
        target: Option<String>,
    ) -> RuntimeHealthStatus {
        match Self::build_runtime_adapter(runtime.clone()) {
            Ok(mut adapter) => match adapter.initialize() {
                Ok(()) => RuntimeHealthStatus {
                    adapter_name: runtime.adapter_name.clone(),
                    binary_path: runtime.binary_path.clone(),
                    healthy: true,
                    target,
                    details: None,
                },
                Err(e) => RuntimeHealthStatus {
                    adapter_name: runtime.adapter_name.clone(),
                    binary_path: runtime.binary_path.clone(),
                    healthy: false,
                    target,
                    details: Some(format!("{}: {}", e.code, e.message)),
                },
            },
            Err(e) => RuntimeHealthStatus {
                adapter_name: runtime.adapter_name.clone(),
                binary_path: runtime.binary_path.clone(),
                healthy: false,
                target,
                details: Some(format!("{}: {}", e.code, e.message)),
            },
        }
    }

    fn build_runtime_adapter(runtime: ProjectRuntimeConfig) -> Result<SelectedRuntimeAdapter> {
        let timeout = Duration::from_millis(runtime.timeout_ms);
        match runtime.adapter_name.as_str() {
            "opencode" => {
                let mut cfg = OpenCodeConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model.clone().or(cfg.model);
                cfg.base.args = runtime.args;
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::OpenCode(
                    crate::adapters::opencode::OpenCodeAdapter::new(cfg),
                ))
            }
            "codex" => {
                let mut cfg = CodexConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = if runtime.args.is_empty() {
                    CodexConfig::default().base.args
                } else {
                    runtime.args
                };
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::Codex(CodexAdapter::new(cfg)))
            }
            "claude-code" => {
                let mut cfg = ClaudeCodeConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = if runtime.args.is_empty() {
                    ClaudeCodeConfig::default().base.args
                } else {
                    runtime.args
                };
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::ClaudeCode(ClaudeCodeAdapter::new(
                    cfg,
                )))
            }
            "kilo" => {
                let mut cfg = KiloConfig::new(PathBuf::from(runtime.binary_path));
                cfg.model = runtime.model;
                cfg.base.args = runtime.args;
                cfg.base.env = runtime.env;
                cfg.base.timeout = timeout;
                Ok(SelectedRuntimeAdapter::Kilo(KiloAdapter::new(cfg)))
            }
            _ => Err(HivemindError::user(
                "unsupported_runtime",
                format!("Unsupported runtime adapter '{}'", runtime.adapter_name),
                "registry:build_runtime_adapter",
            )),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn tick_flow_once(
        &self,
        flow_id: &str,
        interactive: bool,
        preferred_task: Option<Uuid>,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                "registry:tick_flow",
            ));
        }

        let state = self.state()?;
        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
        })?;

        let mut verifying = flow.tasks_in_state(TaskExecState::Verifying);
        verifying.sort();
        if let Some(task_id) = verifying.first().copied() {
            return self.process_verifying_task(flow_id, task_id);
        }

        let mut newly_ready = Vec::new();
        let mut newly_blocked: Vec<(Uuid, String)> = Vec::new();
        for task_id in graph.tasks.keys() {
            let Some(exec) = flow.task_executions.get(task_id) else {
                continue;
            };
            if exec.state != TaskExecState::Pending {
                continue;
            }

            let deps_satisfied = graph.dependencies.get(task_id).is_none_or(|deps| {
                deps.iter().all(|dep| {
                    flow.task_executions
                        .get(dep)
                        .is_some_and(|e| e.state == TaskExecState::Success)
                })
            });

            if deps_satisfied {
                newly_ready.push(*task_id);
            } else {
                let mut missing: Vec<Uuid> = graph
                    .dependencies
                    .get(task_id)
                    .map(|deps| {
                        deps.iter()
                            .filter(|dep| {
                                flow.task_executions
                                    .get(dep)
                                    .is_none_or(|e| e.state != TaskExecState::Success)
                            })
                            .copied()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                missing.sort();

                let preview = missing
                    .iter()
                    .take(5)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                let reason = if missing.len() <= 5 {
                    format!("Waiting on dependencies: {preview}")
                } else {
                    format!(
                        "Waiting on dependencies: {preview} (+{} more)",
                        missing.len().saturating_sub(5)
                    )
                };

                if exec.blocked_reason.as_deref() != Some(reason.as_str()) {
                    newly_blocked.push((*task_id, reason));
                }
            }
        }

        for (task_id, reason) in newly_blocked {
            let event = Event::new(
                EventPayload::TaskBlocked {
                    flow_id: flow.id,
                    task_id,
                    reason: Some(reason),
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;
        }

        for task_id in newly_ready {
            let event = Event::new(
                EventPayload::TaskReady {
                    flow_id: flow.id,
                    task_id,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;
        }

        let flow = self.get_flow(flow_id)?;
        let mut retrying = flow.tasks_in_state(TaskExecState::Retry);
        retrying.sort();
        let mut ready = flow.tasks_in_state(TaskExecState::Ready);
        ready.sort();

        let preferred = preferred_task.filter(|task_id| {
            (retrying.contains(task_id) || ready.contains(task_id))
                && Self::can_auto_run_task(&state, *task_id)
        });
        let task_to_run = preferred.or_else(|| {
            retrying
                .iter()
                .chain(ready.iter())
                .find(|task_id| Self::can_auto_run_task(&state, **task_id))
                .copied()
        });

        let Some(task_id) = task_to_run else {
            let all_success = flow
                .task_executions
                .values()
                .all(|e| e.state == TaskExecState::Success);
            if all_success {
                let event = Event::new(
                    EventPayload::TaskFlowCompleted { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;
                self.maybe_autostart_dependent_flows(flow.id)?;
                return self.get_flow(flow_id);
            }

            return Ok(flow);
        };

        let runtime = Self::effective_runtime_for_task(
            &state,
            &flow,
            task_id,
            RuntimeRole::Worker,
            "registry:tick_flow",
        )?;

        let worktree_status =
            Self::ensure_task_worktree(&flow, &state, task_id, "registry:tick_flow")?;
        let repo_worktrees =
            Self::inspect_task_worktrees(&flow, &state, task_id, "registry:tick_flow")?;

        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:tick_flow",
            )
        })?;

        if exec.state == TaskExecState::Retry && exec.retry_mode == RetryMode::Clean {
            let branch = format!("exec/{}/{task_id}", flow.id);
            for (idx, (_repo_name, repo_worktree)) in repo_worktrees.iter().enumerate() {
                let managers =
                    Self::worktree_managers_for_flow(&flow, &state, "registry:tick_flow")?;
                let (_, manager) = &managers[idx];
                let base = Self::default_base_ref_for_repo(&flow, manager, idx == 0);
                Self::checkout_and_clean_worktree(
                    &repo_worktree.path,
                    &branch,
                    &base,
                    "registry:tick_flow",
                )?;
            }
        }

        let next_attempt_number = exec.attempt_count.saturating_add(1);

        // Ensure this worktree contains the latest changes from dependency tasks.
        // Each task runs in its own worktree/branch (`exec/<flow>/<task>`), so dependent
        // tasks must merge dependency branch heads to see upstream work.
        if let Some(deps) = graph.dependencies.get(&task_id) {
            let mut dep_ids: Vec<Uuid> = deps.iter().copied().collect();
            dep_ids.sort();

            for (_repo_name, repo_worktree) in &repo_worktrees {
                for &dep_task_id in &dep_ids {
                    let dep_branch = format!("exec/{}/{dep_task_id}", flow.id);
                    let dep_ref = format!("refs/heads/{dep_branch}");

                    let ref_exists = std::process::Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["show-ref", "--verify", "--quiet", &dep_ref])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if !ref_exists {
                        continue;
                    }

                    let already_contains = std::process::Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args(["merge-base", "--is-ancestor", &dep_branch, "HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);

                    if already_contains {
                        continue;
                    }

                    let merge = std::process::Command::new("git")
                        .current_dir(&repo_worktree.path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "merge",
                            "--no-edit",
                            &dep_branch,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system(
                                "git_merge_failed",
                                e.to_string(),
                                "registry:tick_flow",
                            )
                        })?;

                    if !merge.status.success() {
                        let _ = std::process::Command::new("git")
                            .current_dir(&repo_worktree.path)
                            .args(["merge", "--abort"])
                            .output();
                        return Err(HivemindError::git(
                            "merge_failed",
                            String::from_utf8_lossy(&merge.stderr).to_string(),
                            "registry:tick_flow",
                        ));
                    }
                }
            }
        }

        // Use the canonical task lifecycle so we persist baselines, compute diffs, and create
        // checkpoint commits. This is required for dependency propagation.
        let attempt_id = self.start_task_execution(&task_id.to_string())?;
        let attempt_corr = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            task_id,
            attempt_id,
        );

        let task = graph.tasks.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_not_found",
                "Task not found in graph",
                "registry:tick_flow",
            )
        })?;

        let max_attempts = task.retry_policy.max_retries.saturating_add(1);
        let checkpoint_ids = Self::normalized_checkpoint_ids(&task.checkpoints);

        let checkpoint_help = if checkpoint_ids.is_empty() {
            None
        } else {
            Some(format!(
                "Execution checkpoints (in order): {}\nComplete checkpoints from the runtime using: \"$HIVEMIND_BIN\" checkpoint complete --id <checkpoint-id> [--summary \"...\"]\n(If available, \"$HIVEMIND_AGENT_BIN\" may be used equivalently.)\nAttempt ID for this run: {attempt_id}",
                checkpoint_ids.join(", ")
            ))
        };
        let repo_context = format!(
            "Multi-repo worktrees for this attempt:\n{}",
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("- {name}: {}", wt.path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let (retry_context, prior_attempts) = if next_attempt_number > 1 {
            let (ctx, priors, ids, req, opt, ec, term) = self.build_retry_context(
                &state,
                &flow,
                task_id,
                next_attempt_number,
                max_attempts,
                "registry:tick_flow",
            )?;

            self.append_event(
                Event::new(
                    EventPayload::RetryContextAssembled {
                        flow_id: flow.id,
                        task_id,
                        attempt_id,
                        attempt_number: next_attempt_number,
                        max_attempts,
                        prior_attempt_ids: ids,
                        required_check_failures: req,
                        optional_check_failures: opt,
                        runtime_exit_code: ec,
                        runtime_terminated_reason: term,
                        context: ctx.clone(),
                    },
                    attempt_corr.clone(),
                ),
                "registry:tick_flow",
            )?;

            let context = checkpoint_help.as_ref().map_or_else(
                || format!("{ctx}\n\n{repo_context}"),
                |checkpoint_text| format!("{ctx}\n\n{repo_context}\n\n{checkpoint_text}"),
            );

            (Some(context), priors)
        } else {
            let context = match checkpoint_help {
                Some(text) => format!("{repo_context}\n\n{text}"),
                None => repo_context,
            };
            (Some(context), Vec::new())
        };

        let input = ExecutionInput {
            task_description: task
                .description
                .clone()
                .unwrap_or_else(|| task.title.clone()),
            success_criteria: task.criteria.description.clone(),
            context: retry_context,
            prior_attempts,
            verifier_feedback: None,
        };
        let runtime_prompt = format_execution_prompt(&input);
        let runtime_flags = Self::runtime_start_flags(&runtime);

        self.store
            .append(Event::new(
                EventPayload::RuntimeStarted {
                    adapter_name: runtime.adapter_name.clone(),
                    task_id,
                    attempt_id,
                    prompt: runtime_prompt,
                    flags: runtime_flags,
                },
                attempt_corr.clone(),
            ))
            .map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;

        let mut runtime_for_adapter = runtime;

        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt_id.to_string());
        let _ = fs::create_dir_all(&target_dir);
        runtime_for_adapter
            .env
            .entry("CARGO_TARGET_DIR".to_string())
            .or_insert_with(|| target_dir.to_string_lossy().to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_ATTEMPT_ID".to_string(), attempt_id.to_string());
        if task.scope.is_some() {
            let trace_path = self.scope_trace_path(attempt_id);
            let _ = fs::create_dir_all(self.scope_traces_dir());
            runtime_for_adapter.env.insert(
                "HIVEMIND_SCOPE_TRACE_LOG".to_string(),
                trace_path.to_string_lossy().to_string(),
            );
        }
        runtime_for_adapter
            .env
            .insert("HIVEMIND_TASK_ID".to_string(), task_id.to_string());
        runtime_for_adapter
            .env
            .insert("HIVEMIND_FLOW_ID".to_string(), flow.id.to_string());
        runtime_for_adapter.env.insert(
            "HIVEMIND_PRIMARY_WORKTREE".to_string(),
            worktree_status.path.to_string_lossy().to_string(),
        );
        runtime_for_adapter.env.insert(
            "HIVEMIND_ALL_WORKTREES".to_string(),
            repo_worktrees
                .iter()
                .map(|(name, wt)| format!("{name}={}", wt.path.display()))
                .collect::<Vec<_>>()
                .join(";"),
        );
        for (repo_name, wt) in &repo_worktrees {
            let env_key = format!(
                "HIVEMIND_REPO_{}_WORKTREE",
                repo_name
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() {
                        c.to_ascii_uppercase()
                    } else {
                        '_'
                    })
                    .collect::<String>()
            );
            runtime_for_adapter
                .env
                .insert(env_key, wt.path.to_string_lossy().to_string());
        }
        if let Ok(bin) = std::env::current_exe() {
            let hivemind_bin = bin.to_string_lossy().to_string();
            runtime_for_adapter
                .env
                .insert("HIVEMIND_BIN".to_string(), hivemind_bin);

            let agent_path = bin
                .parent()
                .map(|p| p.join("hivemind-agent"))
                .filter(|p| p.exists())
                .unwrap_or(bin);
            runtime_for_adapter.env.insert(
                "HIVEMIND_AGENT_BIN".to_string(),
                agent_path.to_string_lossy().to_string(),
            );
        }

        let mut adapter = Self::build_runtime_adapter(runtime_for_adapter.clone())?;
        if let Err(e) = adapter.initialize() {
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e.code,
                &e.message,
                e.recoverable,
                "",
                "",
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }
        if let Err(e) = adapter.prepare(task_id, &worktree_status.path) {
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                &e.code,
                &e.message,
                e.recoverable,
                "",
                "",
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }

        let mut runtime_projector = RuntimeEventProjector::new();

        let (report, terminated_reason) = if interactive {
            let mut stdout = std::io::stdout();

            let res = adapter.execute_interactive(&input, |evt| {
                match evt {
                    InteractiveAdapterEvent::Output { content } => {
                        let chunk = content;
                        let _ = stdout.write_all(chunk.as_bytes());
                        let _ = stdout.flush();
                        let event = Event::new(
                            EventPayload::RuntimeOutputChunk {
                                attempt_id,
                                stream: RuntimeOutputStream::Stdout,
                                content: chunk.clone(),
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                        let _ = self.append_projected_runtime_observations(
                            attempt_id,
                            &attempt_corr,
                            runtime_projector.observe_chunk(RuntimeOutputStream::Stdout, &chunk),
                            "registry:tick_flow",
                        );
                    }
                    InteractiveAdapterEvent::Input { content } => {
                        let event = Event::new(
                            EventPayload::RuntimeInputProvided {
                                attempt_id,
                                content,
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                    InteractiveAdapterEvent::Interrupted => {
                        let event = Event::new(
                            EventPayload::RuntimeInterrupted { attempt_id },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            });

            match res {
                Ok(r) => (r.report, r.terminated_reason),
                Err(e) => {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e.code,
                        &e.message,
                        e.recoverable,
                        "",
                        "",
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }
            }
        } else {
            let report = match adapter.execute(input) {
                Ok(r) => r,
                Err(e) => {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &e.code,
                        &e.message,
                        e.recoverable,
                        "",
                        "",
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }
            };
            (report, None)
        };

        // Best-effort filesystem observed based on the persisted baseline.
        if let Ok(state) = self.state() {
            if let Some(attempt) = state.attempts.get(&attempt_id) {
                if let Some(baseline_id) = attempt.baseline_id {
                    if let Ok(baseline) = self.read_baseline_artifact(baseline_id) {
                        if let Ok(diff) = Diff::compute(&baseline, &worktree_status.path) {
                            let created = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Created)
                                .map(|c| c.path.clone())
                                .collect();
                            let modified = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Modified)
                                .map(|c| c.path.clone())
                                .collect();
                            let deleted = diff
                                .changes
                                .iter()
                                .filter(|c| c.change_type == ChangeType::Deleted)
                                .map(|c| c.path.clone())
                                .collect();

                            let fs_event = Event::new(
                                EventPayload::RuntimeFilesystemObserved {
                                    attempt_id,
                                    files_created: created,
                                    files_modified: modified,
                                    files_deleted: deleted,
                                },
                                attempt_corr.clone(),
                            );
                            let _ = self.store.append(fs_event);
                        }
                    }
                }
            }
        }

        if !interactive {
            for chunk in report.stdout.lines() {
                let content = chunk.to_string();
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stdout,
                        content: content.clone(),
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    &attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stdout, &format!("{content}\n")),
                    "registry:tick_flow",
                );
            }
            for chunk in report.stderr.lines() {
                let content = chunk.to_string();
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stderr,
                        content: content.clone(),
                    },
                    attempt_corr.clone(),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:tick_flow",
                    )
                })?;

                let _ = self.append_projected_runtime_observations(
                    attempt_id,
                    &attempt_corr,
                    runtime_projector
                        .observe_chunk(RuntimeOutputStream::Stderr, &format!("{content}\n")),
                    "registry:tick_flow",
                );
            }
        }

        let _ = self.append_projected_runtime_observations(
            attempt_id,
            &attempt_corr,
            runtime_projector.flush(),
            "registry:tick_flow",
        );

        if let Some(reason) = terminated_reason {
            let _ = self.store.append(Event::new(
                EventPayload::RuntimeTerminated { attempt_id, reason },
                attempt_corr.clone(),
            ));
        }

        let duration_ms = u64::try_from(report.duration.as_millis().min(u128::from(u64::MAX)))
            .unwrap_or(u64::MAX);
        let exited_event = Event::new(
            EventPayload::RuntimeExited {
                attempt_id,
                exit_code: report.exit_code,
                duration_ms,
            },
            attempt_corr,
        );
        self.store.append(exited_event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
        })?;

        if report.exit_code != 0 {
            self.handle_runtime_failure(
                &state,
                &flow,
                task_id,
                attempt_id,
                &runtime_for_adapter,
                next_attempt_number,
                max_attempts,
                "runtime_nonzero_exit",
                &format!("Runtime exited with code {}", report.exit_code),
                true,
                &report.stdout,
                &report.stderr,
                "registry:tick_flow",
            )?;
            return self.get_flow(flow_id);
        }

        if let Err(err) = self.complete_task_execution(&task_id.to_string()) {
            if err.code == "checkpoints_incomplete" {
                if let Some((failure_code, failure_message, recoverable)) =
                    Self::detect_runtime_output_failure(&report.stdout, &report.stderr)
                {
                    self.handle_runtime_failure(
                        &state,
                        &flow,
                        task_id,
                        attempt_id,
                        &runtime_for_adapter,
                        next_attempt_number,
                        max_attempts,
                        &failure_code,
                        &failure_message,
                        recoverable,
                        &report.stdout,
                        &report.stderr,
                        "registry:tick_flow",
                    )?;
                    return self.get_flow(flow_id);
                }

                self.handle_runtime_failure(
                    &state,
                    &flow,
                    task_id,
                    attempt_id,
                    &runtime_for_adapter,
                    next_attempt_number,
                    max_attempts,
                    "checkpoints_incomplete",
                    &err.message,
                    true,
                    &report.stdout,
                    &report.stderr,
                    "registry:tick_flow",
                )?;
                return Err(err);
            }
            return Err(err);
        }

        self.process_verifying_task(flow_id, task_id)
    }

    #[allow(clippy::too_many_lines)]
    pub fn tick_flow(
        &self,
        flow_id: &str,
        interactive: bool,
        max_parallel: Option<u16>,
    ) -> Result<TaskFlow> {
        if interactive {
            return Err(HivemindError::user(
                "interactive_mode_deprecated",
                "Interactive mode is deprecated and no longer supported",
                "registry:tick_flow",
            )
            .with_hint("Re-run without --interactive"));
        }

        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                "registry:tick_flow",
            ));
        }

        let state = self.state()?;
        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:tick_flow",
            )
        })?;

        let flow_defaults = state
            .flow_runtime_defaults
            .get(&flow.id)
            .cloned()
            .unwrap_or_default();
        let configured_limit = flow_defaults
            .worker
            .as_ref()
            .map(|cfg| cfg.max_parallel_tasks.max(1))
            .or_else(|| {
                Self::project_runtime_for_role(project, RuntimeRole::Worker)
                    .map(|cfg| cfg.max_parallel_tasks.max(1))
            })
            .or_else(|| {
                state
                    .global_runtime_defaults
                    .worker
                    .as_ref()
                    .map(|cfg| cfg.max_parallel_tasks.max(1))
            })
            .unwrap_or(1_u16);
        let requested_limit = max_parallel.unwrap_or(configured_limit);
        let global_limit = match env::var("HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL") {
            Ok(raw) => Self::parse_global_parallel_limit(Some(raw))?,
            Err(env::VarError::NotPresent) => Self::parse_global_parallel_limit(None)?,
            Err(env::VarError::NotUnicode(_)) => {
                return Err(HivemindError::user(
                    "invalid_global_parallel_limit",
                    "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be valid UTF-8",
                    "registry:tick_flow",
                ))
            }
        };

        let limit = requested_limit.min(global_limit);
        if limit == 0 {
            return Err(HivemindError::user(
                "invalid_max_parallel",
                "max_parallel must be at least 1",
                "registry:tick_flow",
            )
            .with_hint("Use --max-parallel 1 or higher"));
        }

        let mut started_in_tick: Vec<(Uuid, Scope)> = Vec::new();
        let mut latest_flow = flow;

        for _ in 0..usize::from(limit) {
            let snapshot = self.get_flow(flow_id)?;
            latest_flow = snapshot.clone();
            if snapshot.state != FlowState::Running {
                break;
            }

            let mut verifying = snapshot.tasks_in_state(TaskExecState::Verifying);
            verifying.sort();
            if let Some(task_id) = verifying.first().copied() {
                latest_flow = self.process_verifying_task(flow_id, task_id)?;
                continue;
            }

            let state = self.state()?;
            let graph = state.graphs.get(&snapshot.graph_id).ok_or_else(|| {
                HivemindError::system("graph_not_found", "Graph not found", "registry:tick_flow")
            })?;

            let mut retrying = snapshot.tasks_in_state(TaskExecState::Retry);
            retrying.sort();
            let mut ready = snapshot.tasks_in_state(TaskExecState::Ready);
            ready.sort();

            let mut candidates = retrying;
            for task_id in ready {
                if !candidates.contains(&task_id) {
                    candidates.push(task_id);
                }
            }

            if candidates.is_empty() {
                latest_flow = self.tick_flow_once(flow_id, interactive, None)?;
                break;
            }

            let mut active_scopes = started_in_tick.clone();
            let mut running = snapshot.tasks_in_state(TaskExecState::Running);
            running.sort();
            for running_id in running {
                if active_scopes.iter().any(|(id, _)| *id == running_id) {
                    continue;
                }
                if let Some(task) = graph.tasks.get(&running_id) {
                    active_scopes.push((running_id, task.scope.clone().unwrap_or_default()));
                }
            }

            let mut chosen: Option<(Uuid, Scope)> = None;

            for candidate_id in candidates {
                if !Self::can_auto_run_task(&state, candidate_id) {
                    continue;
                }
                let Some(task) = graph.tasks.get(&candidate_id) else {
                    continue;
                };

                let candidate_scope = task.scope.clone().unwrap_or_default();
                let mut hard_conflict: Option<(Uuid, String)> = None;
                let mut soft_conflict: Option<(Uuid, String)> = None;

                for (other_id, other_scope) in &active_scopes {
                    if *other_id == candidate_id {
                        continue;
                    }

                    match check_compatibility(&candidate_scope, other_scope) {
                        ScopeCompatibility::HardConflict => {
                            hard_conflict = Some((
                                *other_id,
                                format!(
                                    "Hard scope conflict with task {other_id}; serialized in this tick"
                                ),
                            ));
                            break;
                        }
                        ScopeCompatibility::SoftConflict => {
                            if soft_conflict.is_none() {
                                soft_conflict = Some((
                                    *other_id,
                                    format!(
                                        "Soft scope conflict with task {other_id}; allowing parallel attempt with warning"
                                    ),
                                ));
                            }
                        }
                        ScopeCompatibility::Compatible => {}
                    }
                }

                if let Some((conflicting_task_id, reason)) = hard_conflict {
                    self.append_event(
                        Event::new(
                            EventPayload::ScopeConflictDetected {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                conflicting_task_id,
                                severity: "hard_conflict".to_string(),
                                action: "serialized".to_string(),
                                reason: reason.clone(),
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;

                    self.append_event(
                        Event::new(
                            EventPayload::TaskSchedulingDeferred {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                reason,
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;
                    continue;
                }

                if let Some((conflicting_task_id, reason)) = soft_conflict {
                    self.append_event(
                        Event::new(
                            EventPayload::ScopeConflictDetected {
                                flow_id: snapshot.id,
                                task_id: candidate_id,
                                conflicting_task_id,
                                severity: "soft_conflict".to_string(),
                                action: "warn_parallel".to_string(),
                                reason,
                            },
                            CorrelationIds::for_graph_flow_task(
                                snapshot.project_id,
                                snapshot.graph_id,
                                snapshot.id,
                                candidate_id,
                            ),
                        ),
                        "registry:tick_flow",
                    )?;
                }

                chosen = Some((candidate_id, candidate_scope));
                break;
            }

            let Some((task_id, scope)) = chosen else {
                break;
            };

            started_in_tick.push((task_id, scope));
            latest_flow = self.tick_flow_once(flow_id, interactive, Some(task_id))?;
        }

        Ok(latest_flow)
    }

    fn auto_progress_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        const MAX_AUTO_ITERATIONS: usize = 1024;

        for _ in 0..MAX_AUTO_ITERATIONS {
            let latest = self.get_flow(flow_id)?;
            if latest.state != FlowState::Running {
                return Ok(latest);
            }

            let state = self.state()?;
            let graph = state.graphs.get(&latest.graph_id).ok_or_else(|| {
                HivemindError::system(
                    "graph_not_found",
                    "Graph not found",
                    "registry:auto_progress_flow",
                )
            })?;

            let has_verifying = !latest.tasks_in_state(TaskExecState::Verifying).is_empty();
            let has_auto_runnable = latest
                .tasks_in_state(TaskExecState::Retry)
                .into_iter()
                .chain(latest.tasks_in_state(TaskExecState::Ready))
                .any(|task_id| {
                    graph.tasks.contains_key(&task_id) && Self::can_auto_run_task(&state, task_id)
                });

            if !has_verifying && !has_auto_runnable {
                return Ok(latest);
            }

            let before_state = latest.state;
            let before_counts = latest.task_state_counts();
            let next = self.tick_flow(flow_id, false, None)?;
            let after_counts = next.task_state_counts();
            if before_state == next.state && before_counts == after_counts {
                return Ok(next);
            }
        }

        Err(HivemindError::system(
            "auto_progress_limit_exceeded",
            "Auto flow progression exceeded safety iteration limit",
            "registry:auto_progress_flow",
        ))
    }

    fn parse_global_parallel_limit(raw: Option<String>) -> Result<u16> {
        let Some(raw) = raw else {
            return Ok(u16::MAX);
        };

        let parsed = raw.parse::<u16>().map_err(|_| {
            HivemindError::user(
                "invalid_global_parallel_limit",
                format!(
                    "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be a positive integer, got '{raw}'"
                ),
                "registry:tick_flow",
            )
        })?;

        if parsed == 0 {
            return Err(HivemindError::user(
                "invalid_global_parallel_limit",
                "HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL must be at least 1",
                "registry:tick_flow",
            ));
        }

        Ok(parsed)
    }

    /// Returns the registry configuration.
    #[must_use]
    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    /// Attaches a repository to a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found or the path is not a valid git repository.
    pub fn attach_repo(
        &self,
        id_or_name: &str,
        path: &str,
        name: Option<&str>,
        access_mode: RepoAccessMode,
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let path = path.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        let path_buf = std::path::PathBuf::from(path);

        if path.is_empty() {
            let err = HivemindError::user(
                "invalid_repository_path",
                "Repository path cannot be empty",
                "registry:attach_repo",
            )
            .with_hint("Provide a valid filesystem path to a git repository");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        if !path_buf.exists() {
            let err = HivemindError::user(
                "repo_path_not_found",
                format!("Repository path '{path}' not found"),
                "registry:attach_repo",
            )
            .with_hint("Provide an existing path to a git repository");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        // Validate it's a git repository
        let git_dir = path_buf.join(".git");
        if !git_dir.exists() {
            let err = HivemindError::git(
                "not_a_git_repo",
                format!("'{path}' is not a git repository"),
                "registry:attach_repo",
            )
            .with_hint("Provide a path to a directory containing a .git folder");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        // Check if already attached
        let canonical_path = path_buf
            .canonicalize()
            .map_err(|e| {
                let err = HivemindError::system(
                    "path_canonicalize_failed",
                    e.to_string(),
                    "registry:attach_repo",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                err
            })?
            .to_string_lossy()
            .to_string();

        if project
            .repositories
            .iter()
            .any(|r| r.path == canonical_path)
        {
            let err = HivemindError::user(
                "repo_already_attached",
                format!("Repository '{path}' is already attached to this project"),
                "registry:attach_repo",
            )
            .with_hint(
                "Use 'hivemind project inspect <project>' to view attached repos or detach the existing one with 'hivemind project detach-repo <project> <repo-name>'",
            );
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        // Derive repo name from arg or path
        let repo_name = name
            .map(ToString::to_string)
            .or_else(|| {
                path_buf
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "repo".to_string());

        if project.repositories.iter().any(|r| r.name == repo_name) {
            let err = HivemindError::user(
                "repo_name_already_attached",
                format!(
                    "Repository name '{repo_name}' is already attached to project '{}'",
                    project.name
                ),
                "registry:attach_repo",
            )
            .with_hint("Use --name to provide a different repository name");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::RepositoryAttached {
                project_id: project.id,
                path: canonical_path,
                name: repo_name,
                access_mode,
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:attach_repo")
        })?;

        self.get_project(&project.id.to_string())
    }

    /// Detaches a repository from a project.
    ///
    /// # Errors
    /// Returns an error if the project or repository is not found.
    pub fn detach_repo(&self, id_or_name: &str, repo_name: &str) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let state = self.state()?;
        let has_active_flow = state.flows.values().any(|flow| {
            flow.project_id == project.id
                && !matches!(
                    flow.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if has_active_flow {
            let err = HivemindError::user(
                "project_in_active_flow",
                "Cannot detach repositories while project has active flows",
                "registry:detach_repo",
            )
            .with_hint("Abort, complete, or merge all active flows before detaching repositories");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        // Check if repo exists
        if !project.repositories.iter().any(|r| r.name == repo_name) {
            let err = HivemindError::user(
                "repo_not_found",
                format!(
                    "Repository '{repo_name}' is not attached to project '{}'",
                    project.name
                ),
                "registry:detach_repo",
            )
            .with_hint("Use 'hivemind project inspect' to see attached repositories");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::RepositoryDetached {
                project_id: project.id,
                name: repo_name.to_string(),
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:detach_repo")
        })?;

        self.get_project(&project.id.to_string())
    }

    /// Initializes canonical governance storage and projections for a project.
    ///
    /// # Errors
    /// Returns an error if the project cannot be resolved, storage cannot be created,
    /// or governance events cannot be appended.
    pub fn project_governance_init(&self, id_or_name: &str) -> Result<ProjectGovernanceInitResult> {
        let origin = "registry:project_governance_init";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let created_paths = self.ensure_governance_layout(project.id, origin)?;
        let created_set: HashSet<PathBuf> = created_paths.iter().cloned().collect();
        let corr = CorrelationIds::for_project(project.id);

        if !state.governance_projects.contains_key(&project.id) || !created_paths.is_empty() {
            self.append_event(
                Event::new(
                    EventPayload::GovernanceProjectStorageInitialized {
                        project_id: project.id,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                        root_path: self
                            .governance_project_root(project.id)
                            .to_string_lossy()
                            .to_string(),
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut pending_revisions = HashMap::new();
        for location in self.governance_artifact_locations(project.id) {
            let exists = if location.is_dir {
                location.path.is_dir()
            } else {
                location.path.is_file()
            };
            if !exists {
                continue;
            }

            let projected_exists =
                Self::governance_projection_for_location(&state, &location).is_some();
            let should_emit = created_set.contains(&location.path) || !projected_exists;
            if !should_emit {
                continue;
            }

            let revision =
                Self::next_governance_revision(&state, &location, &mut pending_revisions);
            self.append_event(
                Event::new(
                    EventPayload::GovernanceArtifactUpserted {
                        project_id: location.project_id,
                        scope: location.scope.to_string(),
                        artifact_kind: location.artifact_kind.to_string(),
                        artifact_key: location.artifact_key.clone(),
                        path: location.path.to_string_lossy().to_string(),
                        revision,
                        schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                        projection_version: GOVERNANCE_PROJECTION_VERSION,
                    },
                    corr.clone(),
                ),
                origin,
            )?;
        }

        let mut created_paths_rendered: Vec<String> = created_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect();
        created_paths_rendered.sort();

        Ok(ProjectGovernanceInitResult {
            project_id: project.id,
            root_path: self
                .governance_project_root(project.id)
                .to_string_lossy()
                .to_string(),
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
            created_paths: created_paths_rendered,
        })
    }

    /// Migrates legacy governance artifacts from repo-local layout into canonical
    /// global governance storage for the selected project.
    ///
    /// # Errors
    /// Returns an error if migration copy operations fail or governance events
    /// cannot be appended.
    pub fn project_governance_migrate(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceMigrateResult> {
        let origin = "registry:project_governance_migrate";
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut migrated_paths = BTreeSet::new();
        for mapping in self
            .legacy_governance_mappings(&project)
            .into_iter()
            .filter(|m| m.source.exists())
        {
            let copied = if mapping.destination.is_dir {
                Self::copy_dir_if_missing(&mapping.source, &mapping.destination.path, origin)?
            } else {
                Self::copy_file_if_missing(
                    &mapping.source,
                    &mapping.destination.path,
                    Some(Self::governance_default_file_contents(&mapping.destination)),
                    origin,
                )?
            };

            if copied {
                migrated_paths.insert(mapping.destination.path.to_string_lossy().to_string());
            }
        }

        let _ = self.project_governance_init(&project.id.to_string())?;

        let rollback_hint = format!(
            "Rollback by restoring repo-local governance paths from backups under each attached repo '.hivemind/' directory. New layout root: {}",
            self.governance_project_root(project.id).to_string_lossy()
        );
        let migrated_paths_vec: Vec<String> = migrated_paths.into_iter().collect();

        self.append_event(
            Event::new(
                EventPayload::GovernanceStorageMigrated {
                    project_id: Some(project.id),
                    from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
                    to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
                    migrated_paths: migrated_paths_vec.clone(),
                    rollback_hint: rollback_hint.clone(),
                    schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
                    projection_version: GOVERNANCE_PROJECTION_VERSION,
                },
                CorrelationIds::for_project(project.id),
            ),
            origin,
        )?;

        Ok(ProjectGovernanceMigrateResult {
            project_id: project.id,
            from_layout: GOVERNANCE_FROM_LAYOUT.to_string(),
            to_layout: GOVERNANCE_TO_LAYOUT.to_string(),
            migrated_paths: migrated_paths_vec,
            rollback_hint,
            schema_version: GOVERNANCE_SCHEMA_VERSION.to_string(),
            projection_version: GOVERNANCE_PROJECTION_VERSION,
        })
    }

    /// Inspects governance storage and projection status for a project.
    ///
    /// # Errors
    /// Returns an error if the project cannot be resolved or state replay fails.
    pub fn project_governance_inspect(
        &self,
        id_or_name: &str,
    ) -> Result<ProjectGovernanceInspectResult> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let mut artifacts: Vec<GovernanceArtifactInspect> = self
            .governance_artifact_locations(project.id)
            .into_iter()
            .map(|location| {
                let projection = Self::governance_projection_for_location(&state, &location);
                GovernanceArtifactInspect {
                    scope: location.scope.to_string(),
                    artifact_kind: location.artifact_kind.to_string(),
                    artifact_key: location.artifact_key,
                    path: location.path.to_string_lossy().to_string(),
                    exists: if location.is_dir {
                        location.path.is_dir()
                    } else {
                        location.path.is_file()
                    },
                    projected: projection.is_some(),
                    revision: projection.map_or(0, |item| item.revision),
                }
            })
            .collect();
        artifacts.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then(a.artifact_kind.cmp(&b.artifact_kind))
                .then(a.artifact_key.cmp(&b.artifact_key))
        });

        let mut migrations: Vec<GovernanceMigrationSummary> = state
            .governance_migrations
            .iter()
            .filter(|migration| {
                migration.project_id.is_none() || migration.project_id == Some(project.id)
            })
            .map(|migration| GovernanceMigrationSummary {
                from_layout: migration.from_layout.clone(),
                to_layout: migration.to_layout.clone(),
                migrated_paths: migration.migrated_paths.clone(),
                rollback_hint: migration.rollback_hint.clone(),
                schema_version: migration.schema_version.clone(),
                projection_version: migration.projection_version,
                migrated_at: migration.migrated_at,
            })
            .collect();
        migrations.sort_by(|a, b| b.migrated_at.cmp(&a.migrated_at));

        let mut legacy_candidates = BTreeSet::new();
        for mapping in self.legacy_governance_mappings(&project) {
            if mapping.source.exists() {
                legacy_candidates.insert(mapping.source.to_string_lossy().to_string());
            }
        }

        let projected_storage = state.governance_projects.get(&project.id);
        Ok(ProjectGovernanceInspectResult {
            project_id: project.id,
            root_path: projected_storage.map_or_else(
                || {
                    self.governance_project_root(project.id)
                        .to_string_lossy()
                        .to_string()
                },
                |item| item.root_path.clone(),
            ),
            initialized: projected_storage.is_some(),
            schema_version: projected_storage.map_or_else(
                || GOVERNANCE_SCHEMA_VERSION.to_string(),
                |item| item.schema_version.clone(),
            ),
            projection_version: projected_storage.map_or(GOVERNANCE_PROJECTION_VERSION, |item| {
                item.projection_version
            }),
            export_import_boundary: GOVERNANCE_EXPORT_IMPORT_BOUNDARY.to_string(),
            worktree_base_dir: WorktreeConfig::default()
                .base_dir
                .to_string_lossy()
                .to_string(),
            artifacts,
            migrations,
            legacy_candidates: legacy_candidates.into_iter().collect(),
        })
    }

    // ========== Task Management ==========

    /// Creates a new task in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn create_task(
        &self,
        project_id_or_name: &str,
        title: &str,
        description: Option<&str>,
        scope: Option<Scope>,
    ) -> Result<Task> {
        let project = self
            .get_project(project_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if title.trim().is_empty() {
            let err = HivemindError::user(
                "invalid_task_title",
                "Task title cannot be empty",
                "registry:create_task",
            )
            .with_hint("Provide a non-empty task title");
            self.record_error_event(&err, CorrelationIds::for_project(project.id));
            return Err(err);
        }

        let task_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskCreated {
                id: task_id,
                project_id: project.id,
                title: title.to_string(),
                description: description.map(String::from),
                scope,
            },
            CorrelationIds::for_task(project.id, task_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_task")
        })?;

        self.get_task(&task_id.to_string())
    }

    /// Lists tasks in a project.
    ///
    /// # Errors
    /// Returns an error if the project is not found.
    pub fn list_tasks(
        &self,
        project_id_or_name: &str,
        state_filter: Option<TaskState>,
    ) -> Result<Vec<Task>> {
        let project = self.get_project(project_id_or_name)?;
        let state = self.state()?;

        let mut tasks: Vec<_> = state
            .tasks
            .into_values()
            .filter(|t| t.project_id == project.id)
            .filter(|t| state_filter.is_none_or(|s| t.state == s))
            .collect();

        tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(tasks)
    }

    /// Gets a task by ID.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn get_task(&self, task_id: &str) -> Result<Task> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:get_task",
            )
        })?;

        let state = self.state()?;
        state.tasks.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "task_not_found",
                format!("Task '{task_id}' not found"),
                "registry:get_task",
            )
            .with_hint("Use 'hivemind task list <project>' to see available tasks")
        })
    }

    /// Updates a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found.
    pub fn update_task(
        &self,
        task_id: &str,
        title: Option<&str>,
        description: Option<&str>,
    ) -> Result<Task> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        if let Some(new_title) = title {
            if new_title.trim().is_empty() {
                let err = HivemindError::user(
                    "invalid_task_title",
                    "Task title cannot be empty",
                    "registry:update_task",
                )
                .with_hint("Provide a non-empty task title");
                self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
                return Err(err);
            }
        }

        let title = title.filter(|t| *t != task.title);
        let description = description.filter(|d| task.description.as_deref() != Some(*d));

        if title.is_none() && description.is_none() {
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskUpdated {
                id: task.id,
                title: title.map(String::from),
                description: description.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:update_task")
        })?;

        self.get_task(task_id)
    }

    /// Closes a task.
    ///
    /// # Errors
    /// Returns an error if the task is not found or already closed.
    pub fn close_task(&self, task_id: &str, reason: Option<&str>) -> Result<Task> {
        let task = self
            .get_task(task_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let state = self.state()?;
        let in_active_flow = state.flows.values().any(|f| {
            f.task_executions.contains_key(&task.id)
                && !matches!(
                    f.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if in_active_flow {
            let err = HivemindError::user(
                "task_in_active_flow",
                "Task is part of an active flow",
                "registry:close_task",
            );
            self.record_error_event(&err, CorrelationIds::for_task(task.project_id, task.id));
            return Err(err);
        }

        if task.state == TaskState::Closed {
            // Idempotent: closing an already closed task is a no-op.
            return Ok(task);
        }

        let event = Event::new(
            EventPayload::TaskClosed {
                id: task.id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_task(task.project_id, task.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:close_task")
        })?;

        self.get_task(task_id)
    }

    pub fn get_graph(&self, graph_id: &str) -> Result<TaskGraph> {
        let id = Uuid::parse_str(graph_id).map_err(|_| {
            HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                "registry:get_graph",
            )
        })?;

        let state = self.state()?;
        state.graphs.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                "registry:get_graph",
            )
        })
    }

    pub fn create_graph(
        &self,
        project_id_or_name: &str,
        name: &str,
        from_tasks: &[Uuid],
    ) -> Result<TaskGraph> {
        let project = self
            .get_project(project_id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let state = self.state()?;

        let mut tasks_to_add = Vec::new();
        for tid in from_tasks {
            let task = state.tasks.get(tid).cloned().ok_or_else(|| {
                let err = HivemindError::user(
                    "task_not_found",
                    format!("Task '{tid}' not found"),
                    "registry:create_graph",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                err
            })?;
            if task.state != TaskState::Open {
                let err = HivemindError::user(
                    "task_not_open",
                    format!("Task '{tid}' is not open"),
                    "registry:create_graph",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
            tasks_to_add.push(task);
        }

        let graph_id = Uuid::new_v4();
        let event = Event::new(
            EventPayload::TaskGraphCreated {
                graph_id,
                project_id: project.id,
                name: name.to_string(),
                description: None,
            },
            CorrelationIds::for_graph(project.id, graph_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:create_graph",
            )
        })?;

        for task in tasks_to_add {
            let graph_task = GraphTask {
                id: task.id,
                title: task.title,
                description: task.description,
                criteria: SuccessCriteria::new("Done"),
                retry_policy: RetryPolicy::default(),
                checkpoints: vec!["checkpoint-1".to_string()],
                scope: task.scope,
            };
            let event = Event::new(
                EventPayload::TaskAddedToGraph {
                    graph_id,
                    task: graph_task,
                },
                CorrelationIds::for_graph(project.id, graph_id),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system(
                    "event_append_failed",
                    e.to_string(),
                    "registry:create_graph",
                )
            })?;
        }

        self.get_graph(&graph_id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn add_graph_dependency(
        &self,
        graph_id: &str,
        from_task: &str,
        to_task: &str,
    ) -> Result<TaskGraph> {
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let from = Uuid::parse_str(from_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{from_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let to = Uuid::parse_str(to_task).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{to_task}' is not a valid task ID"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        if graph.state != GraphState::Draft {
            let locking_flow_id = state
                .flows
                .values()
                .filter(|flow| flow.graph_id == gid)
                .max_by_key(|flow| flow.updated_at)
                .map(|flow| flow.id);
            let message = locking_flow_id.map_or_else(
                || format!("Graph '{graph_id}' is immutable"),
                |flow_id| format!("Graph '{graph_id}' is immutable (locked by flow '{flow_id}')"),
            );

            let mut err = HivemindError::user(
                "graph_immutable",
                message,
                "registry:add_graph_dependency",
            )
            .with_hint(
                "Create a new graph if you need additional dependencies, or modify task execution in the existing flow",
            );
            if let Some(flow_id) = locking_flow_id {
                err = err.with_context("locking_flow_id", flow_id.to_string());
            }
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if !graph.tasks.contains_key(&from) || !graph.tasks.contains_key(&to) {
            let err = HivemindError::user(
                "task_not_in_graph",
                "One or more tasks are not in the graph",
                "registry:add_graph_dependency",
            )
            .with_hint("Ensure both task IDs were included when the graph was created");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if graph
            .dependencies
            .get(&to)
            .is_some_and(|deps| deps.contains(&from))
        {
            return Ok(graph);
        }

        let mut graph_for_check = graph.clone();
        graph_for_check.add_dependency(to, from).map_err(|e| {
            let err = HivemindError::user(
                "cycle_detected",
                e.to_string(),
                "registry:add_graph_dependency",
            )
            .with_hint("Remove one dependency in the cycle and try again");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            err
        })?;

        let event = Event::new(
            EventPayload::DependencyAdded {
                graph_id: gid,
                from_task: from,
                to_task: to,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:add_graph_dependency",
            )
        })?;

        self.get_graph(graph_id)
    }

    pub fn add_graph_task_check(
        &self,
        graph_id: &str,
        task_id: &str,
        check: crate::core::verification::CheckConfig,
    ) -> Result<TaskGraph> {
        let origin = "registry:add_graph_task_check";
        let gid = Uuid::parse_str(graph_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_graph_id",
                format!("'{graph_id}' is not a valid graph ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        let tid = Uuid::parse_str(task_id).map_err(|_| {
            let err = HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;

        let state = self.state()?;
        let graph = state.graphs.get(&gid).cloned().ok_or_else(|| {
            let err = HivemindError::user(
                "graph_not_found",
                format!("Graph '{graph_id}' not found"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::none());
            err
        })?;
        if graph.state != GraphState::Draft {
            let err = HivemindError::user(
                "graph_immutable",
                format!("Graph '{graph_id}' is immutable"),
                origin,
            )
            .with_hint("Checks can only be added to draft graphs");
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }
        if !graph.tasks.contains_key(&tid) {
            let err = HivemindError::user(
                "task_not_in_graph",
                format!("Task '{task_id}' is not part of graph '{graph_id}'"),
                origin,
            );
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let event = Event::new(
            EventPayload::GraphTaskCheckAdded {
                graph_id: gid,
                task_id: tid,
                check,
            },
            CorrelationIds::for_graph(graph.project_id, gid),
        );
        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        self.get_graph(graph_id)
    }

    fn validate_graph_issues(graph: &TaskGraph) -> Vec<String> {
        fn has_cycle(graph: &TaskGraph) -> bool {
            use std::collections::HashSet;

            fn visit(
                graph: &TaskGraph,
                node: Uuid,
                visited: &mut HashSet<Uuid>,
                stack: &mut HashSet<Uuid>,
            ) -> bool {
                if stack.contains(&node) {
                    return true;
                }
                if visited.contains(&node) {
                    return false;
                }

                visited.insert(node);
                stack.insert(node);

                if let Some(deps) = graph.dependencies.get(&node) {
                    for dep in deps {
                        if visit(graph, *dep, visited, stack) {
                            return true;
                        }
                    }
                }

                stack.remove(&node);
                false
            }

            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            for node in graph.tasks.keys() {
                if visit(graph, *node, &mut visited, &mut stack) {
                    return true;
                }
            }
            false
        }

        if graph.tasks.is_empty() {
            return vec!["Graph must contain at least one task".to_string()];
        }

        for (task_id, deps) in &graph.dependencies {
            if !graph.tasks.contains_key(task_id) {
                return vec![format!("Task not found: {task_id}")];
            }
            for dep in deps {
                if !graph.tasks.contains_key(dep) {
                    return vec![format!("Task not found: {dep}")];
                }
            }
        }

        if has_cycle(graph) {
            return vec!["Cycle detected in task dependencies".to_string()];
        }

        Vec::new()
    }

    pub fn validate_graph(&self, graph_id: &str) -> Result<GraphValidationResult> {
        let graph = self.get_graph(graph_id)?;
        let issues = Self::validate_graph_issues(&graph);
        Ok(GraphValidationResult {
            graph_id: graph.id,
            valid: issues.is_empty(),
            issues,
        })
    }

    pub fn list_flows(&self, project_id_or_name: Option<&str>) -> Result<Vec<TaskFlow>> {
        let project_filter = match project_id_or_name {
            Some(id_or_name) => Some(self.get_project(id_or_name)?.id),
            None => None,
        };

        let state = self.state()?;
        let mut flows: Vec<_> = state
            .flows
            .into_values()
            .filter(|flow| project_filter.is_none_or(|pid| flow.project_id == pid))
            .collect();
        flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
        flows.reverse();
        Ok(flows)
    }

    pub fn get_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let id = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{flow_id}' is not a valid flow ID"),
                "registry:get_flow",
            )
        })?;

        let state = self.state()?;
        state.flows.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{flow_id}' not found"),
                "registry:get_flow",
            )
        })
    }

    pub fn list_attempts(
        &self,
        flow_id: Option<&str>,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AttemptListItem>> {
        let origin = "registry:list_attempts";
        let flow_filter = match flow_id {
            Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
                HivemindError::user(
                    "invalid_flow_id",
                    format!("'{raw}' is not a valid flow ID"),
                    origin,
                )
            })?),
            None => None,
        };
        let task_filter = match task_id {
            Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
                HivemindError::user(
                    "invalid_task_id",
                    format!("'{raw}' is not a valid task ID"),
                    origin,
                )
            })?),
            None => None,
        };

        let state = self.state()?;
        let mut attempts: Vec<_> = state
            .attempts
            .values()
            .filter(|attempt| flow_filter.is_none_or(|id| attempt.flow_id == id))
            .filter(|attempt| task_filter.is_none_or(|id| attempt.task_id == id))
            .cloned()
            .collect();
        attempts.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(attempts
            .into_iter()
            .take(limit)
            .map(|attempt| AttemptListItem {
                attempt_id: attempt.id,
                flow_id: attempt.flow_id,
                task_id: attempt.task_id,
                attempt_number: attempt.attempt_number,
                started_at: attempt.started_at,
                all_checkpoints_completed: attempt.all_checkpoints_completed,
            })
            .collect())
    }

    pub fn list_checkpoints(&self, attempt_id: &str) -> Result<Vec<AttemptCheckpoint>> {
        let attempt_uuid = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "registry:list_checkpoints",
            )
        })?;

        let state = self.state()?;
        let attempt = state
            .attempts
            .get(&attempt_uuid)
            .ok_or_else(|| {
                HivemindError::user(
                    "attempt_not_found",
                    "Attempt not found",
                    "registry:list_checkpoints",
                )
            })?
            .clone();

        Ok(attempt.checkpoints)
    }

    fn unmet_flow_dependencies(state: &AppState, flow: &TaskFlow) -> Vec<Uuid> {
        let mut unmet: Vec<Uuid> = flow
            .depends_on_flows
            .iter()
            .filter(|dep_id| {
                state.flows.get(dep_id).is_none_or(|dep| {
                    !matches!(dep.state, FlowState::Completed | FlowState::Merged)
                })
            })
            .copied()
            .collect();
        unmet.sort();
        unmet
    }

    fn can_auto_run_task(state: &AppState, task_id: Uuid) -> bool {
        state
            .tasks
            .get(&task_id)
            .is_none_or(|task| task.run_mode == RunMode::Auto)
    }

    fn maybe_autostart_dependent_flows(&self, completed_flow_id: Uuid) -> Result<()> {
        let state = self.state()?;
        let mut candidates: Vec<Uuid> = state
            .flows
            .values()
            .filter(|flow| {
                flow.state == FlowState::Created
                    && flow.run_mode == RunMode::Auto
                    && flow.depends_on_flows.contains(&completed_flow_id)
                    && Self::unmet_flow_dependencies(&state, flow).is_empty()
            })
            .map(|flow| flow.id)
            .collect();
        candidates.sort();

        for candidate in candidates {
            let _ = self.start_flow(&candidate.to_string())?;
        }
        Ok(())
    }

    pub fn create_flow(&self, graph_id: &str, name: Option<&str>) -> Result<TaskFlow> {
        let graph = self
            .get_graph(graph_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        let issues = Self::validate_graph_issues(&graph);

        if graph.state == GraphState::Draft {
            let valid = issues.is_empty();
            let event = Event::new(
                EventPayload::TaskGraphValidated {
                    graph_id: graph.id,
                    project_id: graph.project_id,
                    valid,
                    issues: issues.clone(),
                },
                CorrelationIds::for_graph(graph.project_id, graph.id),
            );
            self.store.append(event).map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:create_flow")
            })?;

            if valid {
                let event = Event::new(
                    EventPayload::TaskGraphLocked {
                        graph_id: graph.id,
                        project_id: graph.project_id,
                    },
                    CorrelationIds::for_graph(graph.project_id, graph.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:create_flow",
                    )
                })?;
            }
        }

        if !issues.is_empty() {
            let err = HivemindError::user(
                "graph_invalid",
                "Graph validation failed",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string());
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let state = self.state()?;
        let has_active = state.flows.values().any(|f| {
            f.graph_id == graph.id
                && !matches!(
                    f.state,
                    FlowState::Completed | FlowState::Merged | FlowState::Aborted
                )
        });
        if has_active {
            let err = HivemindError::user(
                "graph_in_use",
                "Graph already used by an active flow",
                "registry:create_flow",
            )
            .with_context("graph_id", graph.id.to_string());
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, graph.id));
            return Err(err);
        }

        let flow_id = Uuid::new_v4();
        let task_ids: Vec<Uuid> = graph.tasks.keys().copied().collect();
        let event = Event::new(
            EventPayload::TaskFlowCreated {
                flow_id,
                graph_id: graph.id,
                project_id: graph.project_id,
                name: name.map(String::from),
                task_ids,
            },
            CorrelationIds::for_graph_flow(graph.project_id, graph.id, flow_id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:create_flow")
        })?;

        self.get_flow(&flow_id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn start_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self
            .get_flow(flow_id)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;
        match flow.state {
            FlowState::Created => {}
            FlowState::Paused => {
                let event = Event::new(
                    EventPayload::TaskFlowResumed { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
                let resumed = self.get_flow(flow_id)?;
                if resumed.run_mode == RunMode::Auto {
                    return self.auto_progress_flow(flow_id);
                }
                return Ok(resumed);
            }
            FlowState::Running => {
                let err = HivemindError::user(
                    "flow_already_running",
                    "Flow is already running",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Completed => {
                let err = HivemindError::user(
                    "flow_completed",
                    "Flow has already completed",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::FrozenForMerge => {
                let err = HivemindError::user(
                    "flow_frozen",
                    "Flow is frozen for merge",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Merged => {
                let err = HivemindError::user(
                    "flow_merged",
                    "Flow has already been merged",
                    "registry:start_flow",
                );
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
            FlowState::Aborted => {
                let err =
                    HivemindError::user("flow_aborted", "Flow was aborted", "registry:start_flow");
                self.record_error_event(
                    &err,
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                );
                return Err(err);
            }
        }

        let state = self.state()?;
        let unmet = Self::unmet_flow_dependencies(&state, &flow);
        if !unmet.is_empty() {
            let preview = unmet
                .iter()
                .take(5)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if unmet.len() > 5 {
                format!(" (+{} more)", unmet.len().saturating_sub(5))
            } else {
                String::new()
            };
            let err = HivemindError::user(
                "flow_dependencies_unmet",
                format!("Flow dependencies are not completed: {preview}{suffix}"),
                "registry:start_flow",
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            );
            return Err(err);
        }

        let base_revision = state
            .projects
            .get(&flow.project_id)
            .and_then(|p| p.repositories.first())
            .and_then(|repo| {
                std::process::Command::new("git")
                    .current_dir(&repo.path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty())
            });

        if let Some(project) = state.projects.get(&flow.project_id) {
            for repo in &project.repositories {
                let repo_head = std::process::Command::new("git")
                    .current_dir(&repo.path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty());
                if let Some(head) = repo_head {
                    let _ = std::process::Command::new("git")
                        .current_dir(&repo.path)
                        .args(["branch", "-f", &format!("flow/{}", flow.id), &head])
                        .output();
                }
            }
        }

        let event = Event::new(
            EventPayload::TaskFlowStarted {
                flow_id: flow.id,
                base_revision,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:start_flow")
        })?;

        if let Some(graph) = state.graphs.get(&flow.graph_id) {
            let ready = graph.root_tasks();
            for task_id in ready {
                let event = Event::new(
                    EventPayload::TaskReady {
                        flow_id: flow.id,
                        task_id,
                    },
                    CorrelationIds::for_graph_flow_task(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        task_id,
                    ),
                );
                self.store.append(event).map_err(|e| {
                    HivemindError::system(
                        "event_append_failed",
                        e.to_string(),
                        "registry:start_flow",
                    )
                })?;
            }
        }

        let started = self.get_flow(flow_id)?;
        if started.run_mode == RunMode::Auto {
            return self.auto_progress_flow(flow_id);
        }
        Ok(started)
    }

    pub fn restart_flow(&self, flow_id: &str, name: Option<&str>, start: bool) -> Result<TaskFlow> {
        let source = self.get_flow(flow_id)?;
        if source.state != FlowState::Aborted {
            return Err(HivemindError::user(
                "flow_not_aborted",
                "Only aborted flows can be restarted",
                "registry:restart_flow",
            )
            .with_hint("Abort the flow first, or create a new flow from the graph"));
        }

        let state = self.state()?;
        let runtime_defaults = state
            .flow_runtime_defaults
            .get(&source.id)
            .cloned()
            .unwrap_or_default();
        let mut dependencies: Vec<_> = source.depends_on_flows.iter().copied().collect();
        dependencies.sort();

        let source_graph_id = source.graph_id;
        let source_run_mode = source.run_mode;
        drop(state);

        let mut restarted = self.create_flow(&source_graph_id.to_string(), name)?;
        let restarted_id = restarted.id.to_string();

        for dep in dependencies {
            restarted = self.flow_add_dependency(&restarted_id, &dep.to_string())?;
        }

        if let Some(config) = runtime_defaults.worker {
            let env_pairs: Vec<String> = config
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            restarted = self.flow_runtime_set(
                &restarted_id,
                RuntimeRole::Worker,
                &config.adapter_name,
                &config.binary_path,
                config.model,
                &config.args,
                &env_pairs,
                config.timeout_ms,
                config.max_parallel_tasks,
            )?;
        }
        if let Some(config) = runtime_defaults.validator {
            let env_pairs: Vec<String> = config
                .env
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect();
            restarted = self.flow_runtime_set(
                &restarted_id,
                RuntimeRole::Validator,
                &config.adapter_name,
                &config.binary_path,
                config.model,
                &config.args,
                &env_pairs,
                config.timeout_ms,
                config.max_parallel_tasks,
            )?;
        }

        if source_run_mode != RunMode::Manual {
            restarted = self.flow_set_run_mode(&restarted_id, source_run_mode)?;
        }

        if start && restarted.state == FlowState::Created {
            restarted = self.start_flow(&restarted_id)?;
        }

        Ok(restarted)
    }

    pub fn worktree_list(&self, flow_id: &str) -> Result<Vec<WorktreeStatus>> {
        let flow = self.get_flow(flow_id)?;
        let state = self.state()?;
        let managers = Self::worktree_managers_for_flow(&flow, &state, "registry:worktree_list")?;

        let mut statuses = Vec::new();
        for (_repo_name, manager) in managers {
            for task_id in flow.task_executions.keys() {
                let status = manager
                    .inspect(flow.id, *task_id)
                    .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_list"))?;
                statuses.push(status);
            }
        }
        Ok(statuses)
    }

    pub fn worktree_inspect(&self, task_id: &str) -> Result<WorktreeStatus> {
        let tid = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:worktree_inspect",
            )
        })?;

        let state = self.state()?;
        let mut candidates: Vec<&TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&tid))
            .collect();

        if candidates.is_empty() {
            return Err(HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:worktree_inspect",
            ));
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let manager = Self::worktree_manager_for_flow(&flow, &state)?;
        manager
            .inspect(flow.id, tid)
            .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_inspect"))
    }

    pub fn worktree_cleanup(
        &self,
        flow_id: &str,
        force: bool,
        dry_run: bool,
    ) -> Result<WorktreeCleanupResult> {
        let flow = self.get_flow(flow_id)?;
        if flow.state == FlowState::Running && !force {
            return Err(HivemindError::user(
                "flow_running_cleanup_requires_force",
                "Flow is running; pass --force to clean up active worktrees",
                "registry:worktree_cleanup",
            ));
        }

        let existing_worktrees = self
            .worktree_list(flow_id)?
            .iter()
            .filter(|status| status.is_worktree)
            .count();

        let state = self.state()?;
        let managers =
            Self::worktree_managers_for_flow(&flow, &state, "registry:worktree_cleanup")?;
        if !dry_run {
            for (_repo_name, manager) in managers {
                manager.cleanup_flow(flow.id).map_err(|e| {
                    Self::worktree_error_to_hivemind(e, "registry:worktree_cleanup")
                })?;
            }
        }

        self.append_event(
            Event::new(
                EventPayload::WorktreeCleanupPerformed {
                    flow_id: flow.id,
                    cleaned_worktrees: existing_worktrees,
                    forced: force,
                    dry_run,
                },
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            ),
            "registry:worktree_cleanup",
        )?;

        Ok(WorktreeCleanupResult {
            flow_id: flow.id,
            cleaned_worktrees: existing_worktrees,
            forced: force,
            dry_run,
        })
    }

    pub fn pause_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;

        match flow.state {
            FlowState::Paused => return Ok(flow),
            FlowState::Running => {}
            _ => {
                return Err(HivemindError::user(
                    "flow_not_running",
                    "Flow is not in running state",
                    "registry:pause_flow",
                ));
            }
        }

        let running_tasks: Vec<Uuid> = flow.tasks_in_state(TaskExecState::Running);
        let event = Event::new(
            EventPayload::TaskFlowPaused {
                flow_id: flow.id,
                running_tasks,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:pause_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn resume_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state != FlowState::Paused {
            return Err(HivemindError::user(
                "flow_not_paused",
                "Flow is not paused",
                "registry:resume_flow",
            ));
        }

        let event = Event::new(
            EventPayload::TaskFlowResumed { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:resume_flow")
        })?;

        let resumed = self.get_flow(flow_id)?;
        if resumed.run_mode == RunMode::Auto {
            return self.auto_progress_flow(flow_id);
        }
        Ok(resumed)
    }

    pub fn abort_flow(
        &self,
        flow_id: &str,
        reason: Option<&str>,
        forced: bool,
    ) -> Result<TaskFlow> {
        let flow = self.get_flow(flow_id)?;
        if flow.state == FlowState::Aborted {
            return Ok(flow);
        }
        if flow.state == FlowState::Completed {
            return Err(HivemindError::user(
                "flow_already_terminal",
                "Flow is completed",
                "registry:abort_flow",
            ));
        }

        let state = self.state()?;
        let mut latest_attempt_ids: HashMap<Uuid, (chrono::DateTime<Utc>, Uuid)> = HashMap::new();
        for attempt in state.attempts.values().filter(|a| a.flow_id == flow.id) {
            latest_attempt_ids
                .entry(attempt.task_id)
                .and_modify(|slot| {
                    if attempt.started_at > slot.0 {
                        *slot = (attempt.started_at, attempt.id);
                    }
                })
                .or_insert((attempt.started_at, attempt.id));
        }

        for (task_id, exec) in &flow.task_executions {
            if !matches!(
                exec.state,
                TaskExecState::Running | TaskExecState::Verifying
            ) {
                continue;
            }

            let attempt_id = latest_attempt_ids.get(task_id).map(|(_, id)| *id);
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionStateChanged {
                        flow_id: flow.id,
                        task_id: *task_id,
                        attempt_id,
                        from: exec.state,
                        to: TaskExecState::Failed,
                    },
                    CorrelationIds::for_graph_flow_task(
                        flow.project_id,
                        flow.graph_id,
                        flow.id,
                        *task_id,
                    ),
                ),
                "registry:abort_flow",
            )?;
            self.append_event(
                Event::new(
                    EventPayload::TaskExecutionFailed {
                        flow_id: flow.id,
                        task_id: *task_id,
                        attempt_id,
                        reason: Some("flow_aborted".to_string()),
                    },
                    attempt_id.map_or_else(
                        || {
                            CorrelationIds::for_graph_flow_task(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                            )
                        },
                        |aid| {
                            CorrelationIds::for_graph_flow_task_attempt(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                                aid,
                            )
                        },
                    ),
                ),
                "registry:abort_flow",
            )?;
        }

        let event = Event::new(
            EventPayload::TaskFlowAborted {
                flow_id: flow.id,
                reason: reason.map(String::from),
                forced,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_flow")
        })?;

        self.get_flow(flow_id)
    }

    pub fn retry_task(
        &self,
        task_id: &str,
        reset_count: bool,
        retry_mode: RetryMode,
    ) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:retry_task",
            )
        })?;

        let state = self.state()?;
        let mut candidates: Vec<TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&id))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:retry_task",
            ));
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:retry_task",
            )
        })?;

        let max_retries = state
            .graphs
            .get(&flow.graph_id)
            .and_then(|g| g.tasks.get(&id))
            .map_or(3, |t| t.retry_policy.max_retries);
        let max_attempts = max_retries.saturating_add(1);
        if !reset_count && exec.attempt_count >= max_attempts {
            return Err(HivemindError::user(
                "retry_limit_exceeded",
                "Retry limit exceeded",
                "registry:retry_task",
            ));
        }

        if exec.state != TaskExecState::Failed && exec.state != TaskExecState::Retry {
            return Err(HivemindError::user(
                "task_not_retriable",
                "Task is not in a retriable state",
                "registry:retry_task",
            ));
        }

        if matches!(retry_mode, RetryMode::Clean) {
            if let Ok(managers) =
                Self::worktree_managers_for_flow(&flow, &state, "registry:retry_task")
            {
                for (idx, (_repo_name, manager)) in managers.iter().enumerate() {
                    let base = Self::default_base_ref_for_repo(&flow, manager, idx == 0);
                    let mut status = manager.inspect(flow.id, id).ok();
                    if status.as_ref().is_none_or(|s| !s.is_worktree) {
                        let _ = manager.create(flow.id, id, Some(&base));
                        status = manager.inspect(flow.id, id).ok();
                    }

                    if let Some(status) = status.filter(|s| s.is_worktree) {
                        let branch = format!("exec/{}/{id}", flow.id);
                        Self::checkout_and_clean_worktree(
                            &status.path,
                            &branch,
                            &base,
                            "registry:retry_task",
                        )?;
                    }
                }
            }
        }

        let event = Event::new(
            EventPayload::TaskRetryRequested {
                task_id: id,
                reset_count,
                retry_mode,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:retry_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn start_task_execution(&self, task_id: &str) -> Result<Uuid> {
        let origin = "registry:start_task_execution";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;
        if flow.state != FlowState::Running {
            return Err(HivemindError::user(
                "flow_not_running",
                "Flow is not in running state",
                origin,
            ));
        }

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Ready && exec.state != TaskExecState::Retry {
            return Err(HivemindError::user(
                "task_not_ready",
                "Task is not ready to start",
                origin,
            ));
        }

        let status = Self::ensure_task_worktree(&flow, &state, id, origin)?;
        let attempt_id = Uuid::new_v4();
        let attempt_number = exec.attempt_count.saturating_add(1);
        let baseline = self.capture_and_store_baseline(&status.path, origin)?;

        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;
        let graph_task = graph.tasks.get(&id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;
        let checkpoint_ids = Self::normalized_checkpoint_ids(&graph_task.checkpoints);
        if graph_task.scope.is_some() {
            self.capture_scope_baseline_for_attempt(&flow, &state, attempt_id)?;
        }

        let corr_task =
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id);
        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            id,
            attempt_id,
        );

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id: Some(attempt_id),
                    from: exec.state,
                    to: TaskExecState::Running,
                },
                corr_task,
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::AttemptStarted {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    attempt_number,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let total = u32::try_from(checkpoint_ids.len()).map_err(|_| {
            HivemindError::system(
                "checkpoint_count_overflow",
                "Checkpoint count exceeds supported range",
                origin,
            )
        })?;

        for (idx, checkpoint_id) in checkpoint_ids.iter().enumerate() {
            let order = u32::try_from(idx.saturating_add(1)).map_err(|_| {
                HivemindError::system(
                    "checkpoint_order_overflow",
                    "Checkpoint order exceeds supported range",
                    origin,
                )
            })?;

            self.append_event(
                Event::new(
                    EventPayload::CheckpointDeclared {
                        flow_id: flow.id,
                        task_id: id,
                        attempt_id,
                        checkpoint_id: checkpoint_id.clone(),
                        order,
                        total,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        if let Some(first_checkpoint_id) = checkpoint_ids.first() {
            self.append_event(
                Event::new(
                    EventPayload::CheckpointActivated {
                        flow_id: flow.id,
                        task_id: id,
                        attempt_id,
                        checkpoint_id: first_checkpoint_id.clone(),
                        order: 1,
                    },
                    corr_attempt.clone(),
                ),
                origin,
            )?;
        }

        self.append_event(
            Event::new(
                EventPayload::TaskExecutionStarted {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    attempt_number,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::BaselineCaptured {
                    flow_id: flow.id,
                    task_id: id,
                    attempt_id,
                    baseline_id: baseline.id,
                    git_head: baseline.git_head.clone(),
                    file_count: baseline.file_count(),
                },
                corr_attempt,
            ),
            origin,
        )?;

        Ok(attempt_id)
    }

    #[allow(clippy::too_many_lines)]
    pub fn checkpoint_complete(
        &self,
        attempt_id: &str,
        checkpoint_id: &str,
        summary: Option<&str>,
    ) -> Result<CheckpointCompletionResult> {
        let origin = "registry:checkpoint_complete";
        let attempt_uuid = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                origin,
            )
        })?;

        let checkpoint_id = checkpoint_id.trim();
        if checkpoint_id.is_empty() {
            return Err(HivemindError::user(
                "invalid_checkpoint_id",
                "Checkpoint ID cannot be empty",
                origin,
            ));
        }

        let state = self.state()?;
        let attempt = state
            .attempts
            .get(&attempt_uuid)
            .ok_or_else(|| HivemindError::user("attempt_not_found", "Attempt not found", origin))?;

        let flow = state.flows.get(&attempt.flow_id).ok_or_else(|| {
            HivemindError::system("flow_not_found", "Flow not found for attempt", origin)
        })?;

        let corr_attempt = CorrelationIds::for_graph_flow_task_attempt(
            flow.project_id,
            flow.graph_id,
            flow.id,
            attempt.task_id,
            attempt.id,
        );

        if flow.state != FlowState::Running {
            let err = HivemindError::user(
                "flow_not_running",
                "Flow is not running; checkpoint completion rejected",
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let exec = flow.task_executions.get(&attempt.task_id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Running {
            let err = HivemindError::user(
                "attempt_not_running",
                "Attempt is not in RUNNING state",
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let graph = state
            .graphs
            .get(&flow.graph_id)
            .ok_or_else(|| HivemindError::system("graph_not_found", "Graph not found", origin))?;
        let graph_task = graph.tasks.get(&attempt.task_id).ok_or_else(|| {
            HivemindError::system("task_not_found", "Task not found in graph", origin)
        })?;

        let checkpoint_ids = Self::normalized_checkpoint_ids(&graph_task.checkpoints);
        let Some((order, total)) = Self::checkpoint_order(&checkpoint_ids, checkpoint_id) else {
            let err = HivemindError::user(
                "checkpoint_not_found",
                format!("Checkpoint '{checkpoint_id}' is not declared for this task"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        };

        let Some(current) = attempt
            .checkpoints
            .iter()
            .find(|cp| cp.checkpoint_id == checkpoint_id)
        else {
            let err = HivemindError::user(
                "checkpoint_not_declared",
                format!("Checkpoint '{checkpoint_id}' has not been declared for this attempt"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        };

        if current.state == AttemptCheckpointState::Completed {
            let err = HivemindError::user(
                "checkpoint_already_completed",
                format!("Checkpoint '{checkpoint_id}' is already completed"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        if current.state != AttemptCheckpointState::Active {
            let err = HivemindError::user(
                "checkpoint_not_active",
                format!("Checkpoint '{checkpoint_id}' is not ACTIVE"),
                origin,
            );
            self.record_error_event(&err, corr_attempt);
            return Err(err);
        }

        let idx = usize::try_from(order.saturating_sub(1)).map_err(|_| {
            HivemindError::system(
                "checkpoint_order_invalid",
                "Checkpoint order conversion failed",
                origin,
            )
        })?;

        let completed_ids: HashSet<&str> = attempt
            .checkpoints
            .iter()
            .filter(|cp| cp.state == AttemptCheckpointState::Completed)
            .map(|cp| cp.checkpoint_id.as_str())
            .collect();
        for prev in checkpoint_ids.iter().take(idx) {
            if !completed_ids.contains(prev.as_str()) {
                let err = HivemindError::user(
                    "checkpoint_order_violation",
                    format!("Cannot complete '{checkpoint_id}' before '{prev}'"),
                    origin,
                );
                self.record_error_event(&err, corr_attempt);
                return Err(err);
            }
        }

        let worktree = Self::inspect_task_worktree(flow, &state, attempt.task_id, origin)?;
        let commit_hash = match Self::create_checkpoint_commit(
            &worktree.path,
            &CheckpointCommitSpec {
                flow_id: flow.id,
                task_id: attempt.task_id,
                attempt_id: attempt.id,
                checkpoint_id,
                order,
                total,
                summary,
            },
            origin,
        ) {
            Ok(hash) => hash,
            Err(err) => {
                self.record_error_event(&err, corr_attempt);
                return Err(err);
            }
        };

        let completed_at = Utc::now();
        let summary_owned = summary.map(str::to_string);
        self.append_event(
            Event::new(
                EventPayload::CheckpointCompleted {
                    flow_id: flow.id,
                    task_id: attempt.task_id,
                    attempt_id: attempt.id,
                    checkpoint_id: checkpoint_id.to_string(),
                    order,
                    commit_hash: commit_hash.clone(),
                    timestamp: completed_at,
                    summary: summary_owned,
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        self.append_event(
            Event::new(
                EventPayload::CheckpointCommitCreated {
                    flow_id: flow.id,
                    task_id: attempt.task_id,
                    attempt_id: attempt.id,
                    commit_sha: commit_hash.clone(),
                },
                corr_attempt.clone(),
            ),
            origin,
        )?;

        let next_checkpoint_id = checkpoint_ids.get(idx.saturating_add(1)).cloned();
        if let Some(next_id) = next_checkpoint_id.as_ref() {
            let next_order = order.saturating_add(1);
            self.append_event(
                Event::new(
                    EventPayload::CheckpointActivated {
                        flow_id: flow.id,
                        task_id: attempt.task_id,
                        attempt_id: attempt.id,
                        checkpoint_id: next_id.clone(),
                        order: next_order,
                    },
                    corr_attempt,
                ),
                origin,
            )?;
        } else {
            self.append_event(
                Event::new(
                    EventPayload::AllCheckpointsCompleted {
                        flow_id: flow.id,
                        task_id: attempt.task_id,
                        attempt_id: attempt.id,
                    },
                    corr_attempt,
                ),
                origin,
            )?;
        }

        Ok(CheckpointCompletionResult {
            flow_id: flow.id,
            task_id: attempt.task_id,
            attempt_id: attempt.id,
            checkpoint_id: checkpoint_id.to_string(),
            order,
            total,
            next_checkpoint_id,
            all_completed: order == total,
            commit_hash,
        })
    }

    pub fn complete_task_execution(&self, task_id: &str) -> Result<TaskFlow> {
        let origin = "registry:complete_task_execution";
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;
        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;
        if exec.state != TaskExecState::Running {
            return Err(HivemindError::user(
                "task_not_running",
                "Task is not in running state",
                origin,
            ));
        }

        let attempt = Self::resolve_latest_attempt_without_diff(&state, flow.id, id, origin)?;

        if !attempt.all_checkpoints_completed {
            let err = HivemindError::user(
                "checkpoints_incomplete",
                "All checkpoints must be completed before task completion",
                origin,
            )
            .with_hint(format!(
                "Complete the active checkpoint via `hivemind checkpoint complete --attempt-id {} --id <checkpoint-id>` before finishing the task attempt",
                attempt.id
            ));
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow_task_attempt(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    id,
                    attempt.id,
                ),
            );
            return Err(err);
        }

        let baseline_id = attempt.baseline_id.ok_or_else(|| {
            HivemindError::system(
                "baseline_not_found",
                "Baseline not found for attempt",
                origin,
            )
        })?;

        let status = Self::inspect_task_worktree(&flow, &state, id, origin)?;
        let artifact =
            self.compute_and_store_diff(baseline_id, &status.path, id, attempt.id, origin)?;

        self.emit_task_execution_completion_events(
            &flow,
            id,
            &attempt,
            CompletionArtifacts {
                baseline_id,
                artifact: &artifact,
                checkpoint_commit_sha: None,
            },
            origin,
        )?;

        let updated = self.get_flow(&flow.id.to_string())?;
        if updated.run_mode == RunMode::Auto {
            return self.auto_progress_flow(&flow.id.to_string());
        }
        Ok(updated)
    }

    pub fn get_attempt(&self, attempt_id: &str) -> Result<AttemptState> {
        let id = Uuid::parse_str(attempt_id).map_err(|_| {
            HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "registry:get_attempt",
            )
        })?;

        let state = self.state()?;
        state.attempts.get(&id).cloned().ok_or_else(|| {
            HivemindError::user(
                "attempt_not_found",
                format!("Attempt '{attempt_id}' not found"),
                "registry:get_attempt",
            )
        })
    }

    pub fn get_attempt_diff(&self, attempt_id: &str) -> Result<Option<String>> {
        let attempt = self.get_attempt(attempt_id)?;
        let Some(diff_id) = attempt.diff_id else {
            return Ok(None);
        };
        let artifact = self.read_diff_artifact(diff_id)?;
        Ok(Some(artifact.unified))
    }

    pub fn abort_task(&self, task_id: &str, reason: Option<&str>) -> Result<TaskFlow> {
        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                "registry:abort_task",
            )
        })?;

        let state = self.state()?;
        let mut candidates: Vec<TaskFlow> = state
            .flows
            .values()
            .filter(|f| f.task_executions.contains_key(&id))
            .cloned()
            .collect();

        if candidates.is_empty() {
            return Err(HivemindError::user(
                "task_not_in_flow",
                "Task is not part of any flow",
                "registry:abort_task",
            ));
        }

        candidates.sort_by_key(|f| std::cmp::Reverse(f.updated_at));
        let flow = candidates[0].clone();

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:abort_task",
            )
        })?;

        if exec.state == TaskExecState::Success {
            return Err(HivemindError::user(
                "task_already_terminal",
                "Task is already successful",
                "registry:abort_task",
            ));
        }

        if exec.state == TaskExecState::Failed {
            return Ok(flow);
        }

        let event = Event::new(
            EventPayload::TaskAborted {
                task_id: id,
                reason: reason.map(String::from),
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_task")
        })?;

        let event = Event::new(
            EventPayload::TaskExecutionFailed {
                flow_id: flow.id,
                task_id: id,
                attempt_id: None,
                reason: Some("aborted".to_string()),
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );
        self.store.append(event).map_err(|e| {
            HivemindError::system("event_append_failed", e.to_string(), "registry:abort_task")
        })?;

        self.get_flow(&flow.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn verify_override(&self, task_id: &str, decision: &str, reason: &str) -> Result<TaskFlow> {
        let origin = "registry:verify_override";

        let id = Uuid::parse_str(task_id).map_err(|_| {
            HivemindError::user(
                "invalid_task_id",
                format!("'{task_id}' is not a valid task ID"),
                origin,
            )
        })?;

        if decision != "pass" && decision != "fail" {
            return Err(HivemindError::user(
                "invalid_decision",
                "Decision must be 'pass' or 'fail'",
                origin,
            ));
        }

        if reason.trim().is_empty() {
            return Err(HivemindError::user(
                "invalid_reason",
                "Reason must be non-empty",
                origin,
            ));
        }

        let user = env::var("HIVEMIND_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty());

        let state = self.state()?;
        let flow = Self::flow_for_task(&state, id, origin)?;

        if matches!(
            flow.state,
            FlowState::Completed
                | FlowState::FrozenForMerge
                | FlowState::Merged
                | FlowState::Aborted
        ) {
            return Err(HivemindError::user(
                "flow_not_active",
                "Cannot override verification for a completed or aborted flow",
                origin,
            ));
        }

        let exec = flow.task_executions.get(&id).ok_or_else(|| {
            HivemindError::system("task_exec_not_found", "Task execution not found", origin)
        })?;

        // Allow overrides both during verification and after automated decisions.
        // This supports overriding failed checks/verifier outcomes (Retry/Failed/Escalated).
        if !matches!(
            exec.state,
            TaskExecState::Verifying
                | TaskExecState::Retry
                | TaskExecState::Failed
                | TaskExecState::Escalated
        ) {
            return Err(HivemindError::user(
                "task_not_overridable",
                "Task is not in an overridable state",
                origin,
            ));
        }

        let event = Event::new(
            EventPayload::HumanOverride {
                task_id: id,
                override_type: "VERIFICATION_OVERRIDE".to_string(),
                decision: decision.to_string(),
                reason: reason.to_string(),
                user,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, id),
        );

        self.store
            .append(event)
            .map_err(|e| HivemindError::system("event_append_failed", e.to_string(), origin))?;

        let updated = self.get_flow(&flow.id.to_string())?;

        if decision == "pass" {
            let frozen_commit_sha = Self::resolve_task_frozen_commit_sha(&updated, &state, id);
            self.emit_task_execution_frozen(&updated, id, frozen_commit_sha, origin)?;

            if let Ok(managers) =
                Self::worktree_managers_for_flow(&updated, &state, "registry:verify_override")
            {
                for (_repo_name, manager) in managers {
                    if manager.config().cleanup_on_success {
                        if let Ok(status) = manager.inspect(updated.id, id) {
                            if status.is_worktree {
                                let _ = manager.remove(&status.path);
                            }
                        }
                    }
                }
            }

            let all_success = updated
                .task_executions
                .values()
                .all(|e| e.state == TaskExecState::Success);
            if all_success {
                let event = Event::new(
                    EventPayload::TaskFlowCompleted {
                        flow_id: updated.id,
                    },
                    CorrelationIds::for_graph_flow(
                        updated.project_id,
                        updated.graph_id,
                        updated.id,
                    ),
                );
                let _ = self.store.append(event);
            }
        }

        self.get_flow(&flow.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn merge_prepare(
        &self,
        flow_id: &str,
        target_branch: Option<&str>,
    ) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_prepare";
        let mut flow = self.get_flow(flow_id)?;

        if !matches!(flow.state, FlowState::Completed | FlowState::FrozenForMerge) {
            let err = HivemindError::user(
                "flow_not_completed",
                "Flow has not completed successfully",
                origin,
            );
            self.record_error_event(
                &err,
                CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
            );
            return Err(err);
        }

        let mut state = self.state()?;
        if let Some(ms) = state.merge_states.get(&flow.id) {
            if ms.status == crate::core::state::MergeStatus::Prepared && ms.conflicts.is_empty() {
                return Ok(ms.clone());
            }
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_prepare", origin)?;

        if flow.state == FlowState::Completed {
            self.append_event(
                Event::new(
                    EventPayload::FlowFrozenForMerge { flow_id: flow.id },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;
            flow = self.get_flow(flow_id)?;
            state = self.state()?;
        }

        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system(
                "graph_not_found",
                "Graph not found",
                "registry:merge_prepare",
            )
        })?;

        let mut conflicts = Vec::new();
        let mut integrated_tasks: Vec<(Uuid, Option<String>)> = Vec::new();
        let mut managers =
            Self::worktree_managers_for_flow(&flow, &state, "registry:merge_prepare")?;

        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                "Project not found",
                "registry:merge_prepare",
            )
        })?;
        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                "registry:merge_prepare",
            ));
        }
        let (_primary_repo_name, manager) = managers.drain(..1).next().ok_or_else(|| {
            HivemindError::user(
                "project_has_no_repo",
                "Project has no attached repository",
                "registry:merge_prepare",
            )
        })?;

        let prepared_target_branch = {
            let repo_path = manager.repo_path();
            let current_branch = std::process::Command::new("git")
                .current_dir(repo_path)
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map_or_else(
                    || "HEAD".to_string(),
                    |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                );

            let main_exists = std::process::Command::new("git")
                .current_dir(repo_path)
                .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            let target = target_branch.map_or_else(
                || {
                    if main_exists {
                        "main".to_string()
                    } else {
                        current_branch
                    }
                },
                ToString::to_string,
            );
            if target == "HEAD" {
                return Err(HivemindError::user(
                    "detached_head",
                    "Cannot prepare merge from detached HEAD",
                    "registry:merge_prepare",
                )
                .with_hint("Re-run with --target <branch> or checkout a branch"));
            }
            let base_ref = target.as_str();

            let merge_branch = format!("integration/{}/prepare", flow.id);
            let merge_path = manager
                .config()
                .base_dir
                .join(flow.id.to_string())
                .join("_integration_prepare");

            if merge_path.exists() {
                let _ = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args([
                        "worktree",
                        "remove",
                        "--force",
                        merge_path.to_str().unwrap_or(""),
                    ])
                    .output();
                let _ = fs::remove_dir_all(&merge_path);
            }

            if let Some(parent) = merge_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    HivemindError::system(
                        "create_dir_failed",
                        e.to_string(),
                        "registry:merge_prepare",
                    )
                })?;
            }

            let add = std::process::Command::new("git")
                .current_dir(manager.repo_path())
                .args([
                    "worktree",
                    "add",
                    "-B",
                    &merge_branch,
                    merge_path.to_str().unwrap_or(""),
                    base_ref,
                ])
                .output()
                .map_err(|e| {
                    HivemindError::system(
                        "git_worktree_add_failed",
                        e.to_string(),
                        "registry:merge_prepare",
                    )
                })?;
            if !add.status.success() {
                return Err(HivemindError::git(
                    "git_worktree_add_failed",
                    String::from_utf8_lossy(&add.stderr).to_string(),
                    "registry:merge_prepare",
                ));
            }

            for task_id in graph.topological_order() {
                if flow
                    .task_executions
                    .get(&task_id)
                    .is_none_or(|e| e.state != TaskExecState::Success)
                {
                    continue;
                }

                let task_branch = format!("exec/{}/{task_id}", flow.id);
                let task_ref = format!("refs/heads/{task_branch}");

                let ref_exists = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["show-ref", "--verify", "--quiet", &task_ref])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !ref_exists {
                    let details = format!("task {task_id}: missing branch '{task_branch}'");
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                    break;
                }

                let _ = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
                    .output();

                if let Some(deps) = graph.dependencies.get(&task_id) {
                    for dep in deps {
                        let dep_branch = format!("exec/{}/{dep}", flow.id);
                        let Some(dep_sha) = Self::resolve_git_ref(&merge_path, &dep_branch) else {
                            let details = format!(
                                "task {task_id}: dependency branch missing for {dep_branch}"
                            );
                            conflicts.push(details.clone());
                            self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                            break;
                        };

                        let contains_dependency = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["merge-base", "--is-ancestor", &dep_sha, &task_branch])
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);
                        if !contains_dependency {
                            let details = format!(
                                "task {task_id}: drift detected (missing prerequisite integrated changes)"
                            );
                            conflicts.push(details.clone());
                            self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                            break;
                        }
                    }

                    if !conflicts.is_empty() {
                        break;
                    }
                }

                let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
                let checkout = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_checkout_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !checkout.status.success() {
                    return Err(HivemindError::git(
                        "git_checkout_failed",
                        String::from_utf8_lossy(&checkout.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                let merge = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .env("GIT_AUTHOR_NAME", "Hivemind")
                    .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                    .env("GIT_COMMITTER_NAME", "Hivemind")
                    .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "merge",
                        "--no-commit",
                        "--no-ff",
                        &task_branch,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_merge_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;

                if !merge.status.success() {
                    let unmerged = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["diff", "--name-only", "--diff-filter=U"])
                        .output()
                        .ok()
                        .filter(|o| o.status.success())
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                        .unwrap_or_default();

                    let details = if unmerged.is_empty() {
                        String::from_utf8_lossy(&merge.stderr).to_string()
                    } else {
                        format!("conflicts in: {unmerged}")
                    };
                    conflicts.push(format!("task {task_id}: {details}"));
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;

                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge", "--abort"])
                        .output();
                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", &merge_branch])
                        .output();
                    break;
                }

                let merge_in_progress = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !merge_in_progress {
                    continue;
                }

                let commit_msg = format!(
                        "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {target}\nVerification-Summary: task_checks_passed\nTimestamp: {}\nHivemind-Version: {}",
                        flow.id,
                        Utc::now().to_rfc3339(),
                        env!("CARGO_PKG_VERSION")
                    );
                let commit = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .env("GIT_AUTHOR_NAME", "Hivemind")
                    .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                    .env("GIT_COMMITTER_NAME", "Hivemind")
                    .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "commit",
                        "-m",
                        &commit_msg,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_commit_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !commit.status.success() {
                    return Err(HivemindError::git(
                        "git_commit_failed",
                        String::from_utf8_lossy(&commit.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                let _ = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
                    .output();
                let promote = std::process::Command::new("git")
                    .current_dir(&merge_path)
                    .args(["merge", "--ff-only", &sandbox_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_merge_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !promote.status.success() {
                    let details = String::from_utf8_lossy(&promote.stderr).trim().to_string();
                    conflicts.push(format!("task {task_id}: {details}"));
                    self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                    break;
                }

                let integrated_sha = Self::resolve_git_ref(&merge_path, "HEAD");
                integrated_tasks.push((task_id, integrated_sha));
            }

            if conflicts.is_empty() {
                let target_dir = self
                    .config
                    .data_dir
                    .join("cargo-target")
                    .join(flow.id.to_string())
                    .join("_integration_prepare")
                    .join("checks");
                let _ = fs::create_dir_all(&target_dir);

                let mut unique_checks: Vec<crate::core::verification::CheckConfig> = Vec::new();
                for task_id in graph.topological_order() {
                    if flow
                        .task_executions
                        .get(&task_id)
                        .is_none_or(|e| e.state != TaskExecState::Success)
                    {
                        continue;
                    }
                    if let Some(task) = graph.tasks.get(&task_id) {
                        for check in &task.criteria.checks {
                            if let Some(existing) = unique_checks
                                .iter_mut()
                                .find(|c| c.name == check.name && c.command == check.command)
                            {
                                existing.required = existing.required || check.required;
                                if existing.timeout_ms.is_none() {
                                    existing.timeout_ms = check.timeout_ms;
                                }
                            } else {
                                unique_checks.push(check.clone());
                            }
                        }
                    }
                }

                for check in &unique_checks {
                    self.append_event(
                        Event::new(
                            EventPayload::MergeCheckStarted {
                                flow_id: flow.id,
                                task_id: None,
                                check_name: check.name.clone(),
                                required: check.required,
                            },
                            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                        ),
                        "registry:merge_prepare",
                    )?;

                    let started = Instant::now();
                    let (exit_code, combined) = match Self::run_check_command(
                        &merge_path,
                        &target_dir,
                        &check.command,
                        check.timeout_ms,
                    ) {
                        Ok((exit_code, output, _timed_out)) => (exit_code, output),
                        Err(e) => (127, e.to_string()),
                    };
                    let duration_ms =
                        u64::try_from(started.elapsed().as_millis().min(u128::from(u64::MAX)))
                            .unwrap_or(u64::MAX);
                    let passed = exit_code == 0;

                    let safe_name = check
                        .name
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                        .collect::<String>();
                    let out_path = target_dir.join(format!("merge_check_{safe_name}.log"));
                    if let Err(e) = fs::write(&out_path, &combined) {
                        let details = format!(
                            "failed to write check output for {} to {}: {}",
                            check.name,
                            out_path.display(),
                            e
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, None, details, origin)?;
                        break;
                    }

                    self.append_event(
                        Event::new(
                            EventPayload::MergeCheckCompleted {
                                flow_id: flow.id,
                                task_id: None,
                                check_name: check.name.clone(),
                                passed,
                                exit_code,
                                output: combined.clone(),
                                duration_ms,
                                required: check.required,
                            },
                            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                        ),
                        "registry:merge_prepare",
                    )?;

                    if check.required && !passed {
                        let details = format!(
                            "required check failed: {} (exit={exit_code}, duration={}ms)",
                            check.name, duration_ms
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, None, details, origin)?;
                        if !combined.trim().is_empty() {
                            let snippet = combined.lines().take(10).collect::<Vec<_>>().join("\n");
                            conflicts.push(format!("check output (first lines): {snippet}"));
                        }
                        break;
                    }
                }
            }

            if conflicts.is_empty() {
                let flow_branch = format!("flow/{}", flow.id);
                let update = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args(["branch", "-f", &flow_branch, &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_branch_update_failed",
                            e.to_string(),
                            "registry:merge_prepare",
                        )
                    })?;
                if !update.status.success() {
                    return Err(HivemindError::git(
                        "git_branch_update_failed",
                        String::from_utf8_lossy(&update.stderr).to_string(),
                        "registry:merge_prepare",
                    ));
                }

                for (task_id, commit_sha) in &integrated_tasks {
                    self.append_event(
                        Event::new(
                            EventPayload::TaskIntegratedIntoFlow {
                                flow_id: flow.id,
                                task_id: *task_id,
                                commit_sha: commit_sha.clone(),
                            },
                            CorrelationIds::for_graph_flow_task(
                                flow.project_id,
                                flow.graph_id,
                                flow.id,
                                *task_id,
                            ),
                        ),
                        origin,
                    )?;
                }
            }

            target
        };

        if conflicts.is_empty() {
            for (repo_name, manager) in managers {
                let merge_branch = format!("integration/{}/prepare", flow.id);
                let merge_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_integration_prepare");

                if merge_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(manager.repo_path())
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            merge_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&merge_path);
                }

                if let Some(parent) = merge_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        HivemindError::system("create_dir_failed", e.to_string(), origin)
                    })?;
                }

                let add = std::process::Command::new("git")
                    .current_dir(manager.repo_path())
                    .args([
                        "worktree",
                        "add",
                        "-B",
                        &merge_branch,
                        merge_path.to_str().unwrap_or(""),
                        &prepared_target_branch,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_worktree_add_failed", e.to_string(), origin)
                    })?;
                if !add.status.success() {
                    let details = format!(
                        "repo {repo_name}: {}",
                        String::from_utf8_lossy(&add.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(&flow, None, details, origin)?;
                    continue;
                }

                for task_id in graph.topological_order() {
                    if flow
                        .task_executions
                        .get(&task_id)
                        .is_none_or(|e| e.state != TaskExecState::Success)
                    {
                        continue;
                    }
                    let task_branch = format!("exec/{}/{task_id}", flow.id);
                    let task_ref = format!("refs/heads/{task_branch}");
                    if !Self::git_ref_exists(&merge_path, &task_ref) {
                        let details = format!("repo {repo_name}: task {task_id}: missing branch");
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
                    let checkout = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_checkout_failed", e.to_string(), origin)
                        })?;
                    if !checkout.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&checkout.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let merge = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "-c",
                            "commit.gpgsign=false",
                            "merge",
                            "--no-commit",
                            "--no-ff",
                            &task_branch,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_merge_failed", e.to_string(), origin)
                        })?;
                    if !merge.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&merge.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        let _ = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["merge", "--abort"])
                            .output();
                        break;
                    }

                    let merge_in_progress = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !merge_in_progress {
                        let _ = std::process::Command::new("git")
                            .current_dir(&merge_path)
                            .args(["checkout", &merge_branch])
                            .output();
                        continue;
                    }

                    let commit_msg = format!(
                        "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {}\nRepository: {}\nTimestamp: {}\nHivemind-Version: {}",
                        flow.id,
                        prepared_target_branch,
                        repo_name,
                        Utc::now().to_rfc3339(),
                        env!("CARGO_PKG_VERSION")
                    );
                    let commit = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args([
                            "-c",
                            "user.name=Hivemind",
                            "-c",
                            "user.email=hivemind@example.com",
                            "-c",
                            "commit.gpgsign=false",
                            "commit",
                            "-m",
                            &commit_msg,
                        ])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_commit_failed", e.to_string(), origin)
                        })?;
                    if !commit.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&commit.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }

                    let _ = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", &merge_branch])
                        .output();
                    let promote = std::process::Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge", "--ff-only", &sandbox_branch])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_merge_failed", e.to_string(), origin)
                        })?;
                    if !promote.status.success() {
                        let details = format!(
                            "repo {repo_name}: task {task_id}: {}",
                            String::from_utf8_lossy(&promote.stderr).trim()
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(&flow, Some(task_id), details, origin)?;
                        break;
                    }
                }

                if conflicts.is_empty() {
                    let flow_branch = format!("flow/{}", flow.id);
                    let _ = std::process::Command::new("git")
                        .current_dir(manager.repo_path())
                        .args(["branch", "-f", &flow_branch, &merge_branch])
                        .output();
                }
            }
        }

        let event = Event::new(
            EventPayload::MergePrepared {
                flow_id: flow.id,
                target_branch: Some(prepared_target_branch),
                conflicts,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_prepare",
            )
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after prepare",
                "registry:merge_prepare",
            )
        })
    }

    pub fn merge_approve(&self, flow_id: &str) -> Result<crate::core::state::MergeState> {
        let flow = self.get_flow(flow_id)?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                "registry:merge_approve",
            )
        })?;

        if ms.status == crate::core::state::MergeStatus::Approved {
            return Ok(ms.clone());
        }

        if !ms.conflicts.is_empty() {
            return Err(HivemindError::user(
                "unresolved_conflicts",
                "Merge has unresolved conflicts",
                "registry:merge_approve",
            ));
        }

        let user = env::var("HIVEMIND_USER")
            .or_else(|_| env::var("USER"))
            .ok()
            .filter(|u| !u.trim().is_empty());

        let event = Event::new(
            EventPayload::MergeApproved {
                flow_id: flow.id,
                user,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_approve",
            )
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after approve",
                "registry:merge_approve",
            )
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn merge_execute(&self, flow_id: &str) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_execute";
        let flow = self.get_flow(flow_id)?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                "registry:merge_execute",
            )
        })?;

        if ms.status != crate::core::state::MergeStatus::Approved {
            return Err(HivemindError::user(
                "merge_not_approved",
                "Merge has not been approved",
                "registry:merge_execute",
            ));
        }

        if flow.state != FlowState::FrozenForMerge {
            return Err(HivemindError::user(
                "flow_not_frozen_for_merge",
                "Flow must be frozen for merge before execution",
                origin,
            ));
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_execute", origin)?;

        let mut commits = Vec::new();
        if state.projects.contains_key(&flow.project_id) {
            let managers = Self::worktree_managers_for_flow(&flow, &state, origin)?;
            let merge_branch = format!("integration/{}/prepare", flow.id);
            let merge_ref = format!("refs/heads/{merge_branch}");

            let mut repo_merge_meta: Vec<(String, PathBuf, String, String, WorktreeManager)> =
                Vec::new();
            for (repo_name, manager) in managers {
                let repo_path = manager.repo_path().to_path_buf();
                let dirty = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["status", "--porcelain"])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_status_failed", e.to_string(), origin)
                    })?;
                if !dirty.status.success() {
                    return Err(HivemindError::git(
                        "git_status_failed",
                        String::from_utf8_lossy(&dirty.stderr).to_string(),
                        origin,
                    ));
                }
                let has_dirty_files = String::from_utf8_lossy(&dirty.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .any(|l| {
                        let path = l
                            .strip_prefix("?? ")
                            .or_else(|| l.get(3..))
                            .unwrap_or("")
                            .trim();
                        !path.starts_with(".hivemind/")
                    });
                if has_dirty_files {
                    return Err(HivemindError::user(
                        "repo_dirty",
                        format!("Repository '{repo_name}' has uncommitted changes"),
                        origin,
                    ));
                }

                let current_branch = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let target = ms
                    .target_branch
                    .clone()
                    .unwrap_or_else(|| current_branch.clone());
                if target == "HEAD" {
                    return Err(HivemindError::user(
                        "detached_head",
                        format!("Repository '{repo_name}' is in detached HEAD"),
                        origin,
                    ));
                }
                if !Self::git_ref_exists(&repo_path, &merge_ref) {
                    return Err(HivemindError::user(
                        "merge_branch_not_found",
                        format!("Prepared integration branch not found in repo '{repo_name}'"),
                        origin,
                    )
                    .with_hint("Run 'hivemind merge prepare' again"));
                }

                let ff_possible = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args(["merge-base", "--is-ancestor", &target, &merge_branch])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !ff_possible {
                    return Err(HivemindError::git(
                        "git_merge_failed",
                        format!("Fast-forward is not possible in repo '{repo_name}'"),
                        origin,
                    ));
                }

                repo_merge_meta.push((repo_name, repo_path, current_branch, target, manager));
            }

            let mut merged: Vec<(PathBuf, String, String)> = Vec::new();
            for (repo_name, repo_path, current_branch, target, manager) in &repo_merge_meta {
                if current_branch != target {
                    let checkout = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", target])
                        .output()
                        .map_err(|e| {
                            HivemindError::system("git_checkout_failed", e.to_string(), origin)
                        })?;
                    if !checkout.status.success() {
                        return Err(HivemindError::git(
                            "git_checkout_failed",
                            String::from_utf8_lossy(&checkout.stderr).to_string(),
                            origin,
                        ));
                    }
                }

                let old_head = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let merge_out = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["merge", "--ff-only", &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_merge_failed", e.to_string(), origin)
                    })?;
                if !merge_out.status.success() {
                    for (merged_repo, rollback_head, checkout_back) in merged.iter().rev() {
                        let _ = std::process::Command::new("git")
                            .current_dir(merged_repo)
                            .args(["reset", "--hard", rollback_head])
                            .output();
                        let _ = std::process::Command::new("git")
                            .current_dir(merged_repo)
                            .args(["checkout", checkout_back])
                            .output();
                    }
                    return Err(HivemindError::git(
                        "git_merge_failed",
                        format!(
                            "Merge failed in repo '{repo_name}': {}",
                            String::from_utf8_lossy(&merge_out.stderr).trim()
                        ),
                        origin,
                    ));
                }

                let new_head = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map_or_else(
                        || "HEAD".to_string(),
                        |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
                    );
                let rev_list = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["rev-list", "--reverse", &format!("{old_head}..{new_head}")])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                if !rev_list.is_empty() {
                    commits.extend(
                        rev_list
                            .lines()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                    );
                }
                merged.push((repo_path.clone(), old_head, current_branch.clone()));

                if current_branch != target {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", current_branch])
                        .output();
                }

                let merge_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_merge");
                if merge_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            merge_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&merge_path);
                }
                let prepare_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_integration_prepare");
                if prepare_path.exists() {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args([
                            "worktree",
                            "remove",
                            "--force",
                            prepare_path.to_str().unwrap_or(""),
                        ])
                        .output();
                    let _ = fs::remove_dir_all(&prepare_path);
                }
                let prepare_branch = format!("integration/{}/prepare", flow.id);
                let _ = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["branch", "-D", &prepare_branch])
                    .output();
                if manager.config().cleanup_on_success {
                    for task_id in flow.task_executions.keys() {
                        let branch = format!("exec/{}/{task_id}", flow.id);
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &branch])
                            .output();
                        let integration_branch = format!("integration/{}/{task_id}", flow.id);
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &integration_branch])
                            .output();
                    }
                    let flow_branch = format!("flow/{}", flow.id);
                    if current_branch != &flow_branch {
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &flow_branch])
                            .output();
                    }
                }
            }
        }

        let event = Event::new(
            EventPayload::MergeCompleted {
                flow_id: flow.id,
                commits,
            },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:merge_execute",
            )
        })?;

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after execute",
                "registry:merge_execute",
            )
        })
    }

    pub fn merge_execute_with_options(
        &self,
        flow_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<crate::core::state::MergeState> {
        match options.mode {
            MergeExecuteMode::Local => self.merge_execute(flow_id),
            MergeExecuteMode::Pr => self.merge_execute_via_pr(flow_id, options),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn merge_execute_via_pr(
        &self,
        flow_id: &str,
        options: MergeExecuteOptions,
    ) -> Result<crate::core::state::MergeState> {
        let origin = "registry:merge_execute_via_pr";
        let flow = self.get_flow(flow_id)?;

        let state = self.state()?;
        let ms = state.merge_states.get(&flow.id).ok_or_else(|| {
            HivemindError::user(
                "merge_not_prepared",
                "No merge preparation exists for this flow",
                origin,
            )
        })?;

        if ms.status != crate::core::state::MergeStatus::Approved {
            return Err(HivemindError::user(
                "merge_not_approved",
                "Merge has not been approved",
                origin,
            ));
        }
        if flow.state != FlowState::FrozenForMerge {
            return Err(HivemindError::user(
                "flow_not_frozen_for_merge",
                "Flow must be frozen for merge before execution",
                origin,
            ));
        }

        let _integration_lock = self.acquire_flow_integration_lock(flow.id, origin)?;
        self.emit_integration_lock_acquired(&flow, "merge_execute_pr", origin)?;

        let managers = Self::worktree_managers_for_flow(&flow, &state, origin)?;
        if managers.len() != 1 {
            return Err(HivemindError::user(
                "pr_merge_multi_repo_unsupported",
                "PR merge mode currently supports exactly one repository",
                origin,
            ));
        }
        let (_repo_name, manager) = managers.into_iter().next().ok_or_else(|| {
            HivemindError::system(
                "repo_not_found",
                "No repository attached to project",
                origin,
            )
        })?;
        let repo_path = manager.repo_path().to_path_buf();
        let merge_branch = format!("integration/{}/prepare", flow.id);
        let merge_ref = format!("refs/heads/{merge_branch}");
        if !Self::git_ref_exists(&repo_path, &merge_ref) {
            return Err(HivemindError::user(
                "merge_branch_not_found",
                "Prepared integration branch not found",
                origin,
            )
            .with_hint("Run 'hivemind merge prepare' again"));
        }

        let target_branch = ms
            .target_branch
            .clone()
            .unwrap_or_else(|| "main".to_string());
        let old_target_head = std::process::Command::new("git")
            .current_dir(&repo_path)
            .args(["rev-parse", &target_branch])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let push = std::process::Command::new("git")
            .current_dir(&repo_path)
            .args(["push", "--set-upstream", "origin", &merge_branch])
            .output()
            .map_err(|e| HivemindError::system("git_push_failed", e.to_string(), origin))?;
        if !push.status.success() {
            return Err(HivemindError::git(
                "git_push_failed",
                String::from_utf8_lossy(&push.stderr).to_string(),
                origin,
            ));
        }

        let title = format!("Hivemind flow {} merge", flow.id);
        let body = format!(
            "Automated merge PR for flow {}\n\nTarget: {}\n\nGenerated by Hivemind.",
            flow.id, target_branch
        );

        let create = std::process::Command::new("gh")
            .current_dir(&repo_path)
            .args([
                "pr",
                "create",
                "--base",
                &target_branch,
                "--head",
                &merge_branch,
                "--title",
                &title,
                "--body",
                &body,
            ])
            .output()
            .map_err(|e| HivemindError::system("gh_not_available", e.to_string(), origin))?;

        if !create.status.success() {
            let list_existing = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args([
                    "pr",
                    "list",
                    "--state",
                    "open",
                    "--base",
                    &target_branch,
                    "--head",
                    &merge_branch,
                    "--json",
                    "number",
                    "--jq",
                    ".[0].number",
                ])
                .output()
                .map_err(|e| HivemindError::system("gh_pr_list_failed", e.to_string(), origin))?;

            let existing = String::from_utf8_lossy(&list_existing.stdout)
                .trim()
                .to_string();
            if existing.is_empty() {
                return Err(HivemindError::system(
                    "gh_pr_create_failed",
                    String::from_utf8_lossy(&create.stderr).to_string(),
                    origin,
                ));
            }
        }

        let pr_number_out = std::process::Command::new("gh")
            .current_dir(&repo_path)
            .args(["pr", "view", "--json", "number", "--jq", ".number"])
            .output()
            .map_err(|e| HivemindError::system("gh_pr_view_failed", e.to_string(), origin))?;
        if !pr_number_out.status.success() {
            return Err(HivemindError::system(
                "gh_pr_view_failed",
                String::from_utf8_lossy(&pr_number_out.stderr).to_string(),
                origin,
            ));
        }
        let pr_number = String::from_utf8_lossy(&pr_number_out.stdout)
            .trim()
            .to_string();
        if pr_number.is_empty() {
            return Err(HivemindError::system(
                "gh_pr_view_failed",
                "Unable to resolve PR number".to_string(),
                origin,
            ));
        }

        if options.monitor_ci {
            let checks = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args(["pr", "checks", &pr_number, "--watch", "--required"])
                .output()
                .map_err(|e| HivemindError::system("gh_pr_checks_failed", e.to_string(), origin))?;
            if !checks.status.success() {
                return Err(HivemindError::system(
                    "gh_pr_checks_failed",
                    String::from_utf8_lossy(&checks.stderr).to_string(),
                    origin,
                ));
            }
        }

        let merged_now = if options.auto_merge {
            let mut args = vec!["pr", "merge", &pr_number, "--squash", "--delete-branch"];
            if !options.monitor_ci {
                args.push("--auto");
            }
            let merge = std::process::Command::new("gh")
                .current_dir(&repo_path)
                .args(args)
                .output()
                .map_err(|e| HivemindError::system("gh_pr_merge_failed", e.to_string(), origin))?;
            if !merge.status.success() {
                return Err(HivemindError::system(
                    "gh_pr_merge_failed",
                    String::from_utf8_lossy(&merge.stderr).to_string(),
                    origin,
                ));
            }
            options.monitor_ci
        } else {
            false
        };

        if options.pull_after && merged_now {
            let checkout = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["checkout", &target_branch])
                .output()
                .map_err(|e| HivemindError::system("git_checkout_failed", e.to_string(), origin))?;
            if !checkout.status.success() {
                return Err(HivemindError::git(
                    "git_checkout_failed",
                    String::from_utf8_lossy(&checkout.stderr).to_string(),
                    origin,
                ));
            }

            let pull = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["pull", "--ff-only", "origin", &target_branch])
                .output()
                .map_err(|e| HivemindError::system("git_pull_failed", e.to_string(), origin))?;
            if !pull.status.success() {
                return Err(HivemindError::git(
                    "git_pull_failed",
                    String::from_utf8_lossy(&pull.stderr).to_string(),
                    origin,
                ));
            }
        }

        if merged_now {
            let new_target_head = std::process::Command::new("git")
                .current_dir(&repo_path)
                .args(["rev-parse", &target_branch])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

            let mut commits = Vec::new();
            if let Some(old_head) = old_target_head {
                let rev_list = std::process::Command::new("git")
                    .current_dir(&repo_path)
                    .args([
                        "rev-list",
                        "--reverse",
                        &format!("{old_head}..{new_target_head}"),
                    ])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                if !rev_list.is_empty() {
                    commits.extend(
                        rev_list
                            .lines()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                    );
                }
            } else if !new_target_head.is_empty() {
                commits.push(new_target_head);
            }

            self.append_event(
                Event::new(
                    EventPayload::MergeCompleted {
                        flow_id: flow.id,
                        commits,
                    },
                    CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
                ),
                origin,
            )?;
        }

        let state = self.state()?;
        state.merge_states.get(&flow.id).cloned().ok_or_else(|| {
            HivemindError::system(
                "merge_state_not_found",
                "Merge state not found after PR execution",
                origin,
            )
        })
    }

    pub fn replay_flow(&self, flow_id: &str) -> Result<TaskFlow> {
        let fid = Uuid::parse_str(flow_id).map_err(|_| {
            HivemindError::user(
                "invalid_flow_id",
                format!("'{flow_id}' is not a valid flow ID"),
                "registry:replay_flow",
            )
        })?;

        let filter = EventFilter {
            flow_id: Some(fid),
            ..EventFilter::default()
        };
        let events = self.read_events(&filter)?;
        if events.is_empty() {
            return Err(HivemindError::user(
                "flow_not_found",
                format!("No events found for flow '{flow_id}'"),
                "registry:replay_flow",
            ));
        }

        let all_events = self.store.read_all().map_err(|e| {
            HivemindError::system("event_read_failed", e.to_string(), "registry:replay_flow")
        })?;
        let flow_related: Vec<Event> = all_events
            .into_iter()
            .filter(|e| {
                e.metadata.correlation.flow_id == Some(fid)
                    || match &e.payload {
                        EventPayload::TaskFlowCreated { flow_id: f, .. } => *f == fid,
                        _ => false,
                    }
            })
            .collect();

        let replayed = crate::core::state::AppState::replay(&flow_related);
        replayed.flows.get(&fid).cloned().ok_or_else(|| {
            HivemindError::user(
                "flow_not_found",
                format!("Flow '{flow_id}' not found in replayed state"),
                "registry:replay_flow",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scope::{FilePermission, FilesystemScope, PathRule};
    use crate::storage::event_store::InMemoryEventStore;
    use std::process::Command;

    fn test_registry() -> Registry {
        let store = Arc::new(InMemoryEventStore::new());
        let data_dir =
            std::env::temp_dir().join(format!("hivemind-registry-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&data_dir).expect("create test registry data dir");
        let config = RegistryConfig::with_dir(data_dir);
        Registry::with_store(store, config)
    }

    fn init_git_repo(repo_dir: &std::path::Path) {
        std::fs::create_dir_all(repo_dir).expect("create repo dir");

        let out = Command::new("git")
            .args(["init"])
            .current_dir(repo_dir)
            .output()
            .expect("git init");
        assert!(
            out.status.success(),
            "git init: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");

        let out = Command::new("git")
            .args(["add", "."])
            .current_dir(repo_dir)
            .output()
            .expect("git add");
        assert!(
            out.status.success(),
            "git add: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let out = Command::new("git")
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                "init",
            ])
            .current_dir(repo_dir)
            .output()
            .expect("git commit");
        assert!(
            out.status.success(),
            "git commit: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let _ = Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(repo_dir)
            .output();
    }

    fn configure_failing_runtime(registry: &Registry) {
        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo runtime_started; exit 1".to_string(),
                ],
                &[],
                1000,
                4,
            )
            .unwrap();
    }

    #[test]
    fn create_and_list_projects() {
        let registry = test_registry();

        registry.create_project("project-a", None).unwrap();
        registry
            .create_project("project-b", Some("Description"))
            .unwrap();

        let projects = registry.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "project-a");
        assert_eq!(projects[1].name, "project-b");
    }

    #[test]
    fn duplicate_project_name_rejected() {
        let registry = test_registry();

        registry.create_project("test", None).unwrap();
        let result = registry.create_project("test", None);

        assert!(result.is_err());
    }

    #[test]
    fn get_project_by_name() {
        let registry = test_registry();

        let created = registry.create_project("my-project", None).unwrap();
        let found = registry.get_project("my-project").unwrap();

        assert_eq!(created.id, found.id);
    }

    #[test]
    fn get_project_by_id() {
        let registry = test_registry();

        let created = registry.create_project("my-project", None).unwrap();
        let found = registry.get_project(&created.id.to_string()).unwrap();

        assert_eq!(created.id, found.id);
    }

    #[test]
    fn update_project() {
        let registry = test_registry();

        registry.create_project("old-name", None).unwrap();
        let updated = registry
            .update_project("old-name", Some("new-name"), Some("New desc"))
            .unwrap();

        assert_eq!(updated.name, "new-name");
        assert_eq!(updated.description, Some("New desc".to_string()));
    }

    #[test]
    fn project_governance_init_creates_layout_and_projection_state() {
        let registry = test_registry();
        let project = registry.create_project("proj", None).unwrap();

        let result = registry.project_governance_init("proj").unwrap();

        assert_eq!(result.project_id, project.id);
        assert!(!result.created_paths.is_empty());
        assert!(std::path::Path::new(&result.root_path).is_dir());

        let state = registry.state().unwrap();
        assert!(state.governance_projects.contains_key(&project.id));
        assert!(state.governance_artifacts.values().any(|artifact| {
            artifact.project_id == Some(project.id) && artifact.artifact_kind == "constitution"
        }));
    }

    #[test]
    fn project_governance_migrate_copies_legacy_artifacts_and_emits_migration_event() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        let project = registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_dir.to_string_lossy().as_ref(),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let legacy_constitution = repo_dir.join(".hivemind").join("constitution.yaml");
        let legacy_global_notepad = repo_dir.join(".hivemind").join("global").join("notepad.md");
        std::fs::create_dir_all(legacy_constitution.parent().unwrap()).unwrap();
        std::fs::create_dir_all(legacy_global_notepad.parent().unwrap()).unwrap();
        std::fs::write(&legacy_constitution, "legacy_constitution: true\n").unwrap();
        std::fs::write(&legacy_global_notepad, "legacy global notes\n").unwrap();

        let result = registry.project_governance_migrate("proj").unwrap();

        assert_eq!(result.project_id, project.id);
        assert!(!result.migrated_paths.is_empty());
        assert!(result
            .migrated_paths
            .iter()
            .any(|path| path.ends_with("constitution.yaml")));

        let canonical_constitution = registry
            .config()
            .data_dir
            .join("projects")
            .join(project.id.to_string())
            .join("constitution.yaml");
        let canonical_global_notepad = registry.config().data_dir.join("global").join("notepad.md");
        assert_eq!(
            std::fs::read_to_string(&canonical_constitution).unwrap(),
            "legacy_constitution: true\n"
        );
        assert_eq!(
            std::fs::read_to_string(&canonical_global_notepad).unwrap(),
            "legacy global notes\n"
        );

        let inspect = registry.project_governance_inspect("proj").unwrap();
        assert!(inspect.initialized);
        assert!(inspect
            .migrations
            .iter()
            .any(|migration| migration.to_layout == GOVERNANCE_TO_LAYOUT));
    }

    #[test]
    fn list_graphs_and_flows_support_project_filters() {
        let registry = test_registry();
        registry.create_project("proj-a", None).unwrap();
        registry.create_project("proj-b", None).unwrap();

        let a_task = registry
            .create_task("proj-a", "Task A", None, None)
            .unwrap();
        let b_task = registry
            .create_task("proj-b", "Task B", None, None)
            .unwrap();

        let a_graph = registry
            .create_graph("proj-a", "graph-a", &[a_task.id])
            .unwrap();
        let b_graph = registry
            .create_graph("proj-b", "graph-b", &[b_task.id])
            .unwrap();

        let a_flow = registry
            .create_flow(&a_graph.id.to_string(), Some("flow-a"))
            .unwrap();
        let b_flow = registry
            .create_flow(&b_graph.id.to_string(), Some("flow-b"))
            .unwrap();

        let graphs_a = registry.list_graphs(Some("proj-a")).unwrap();
        assert_eq!(graphs_a.len(), 1);
        assert_eq!(graphs_a[0].id, a_graph.id);

        let graphs_b = registry.list_graphs(Some("proj-b")).unwrap();
        assert_eq!(graphs_b.len(), 1);
        assert_eq!(graphs_b[0].id, b_graph.id);

        let flows_a = registry.list_flows(Some("proj-a")).unwrap();
        assert_eq!(flows_a.len(), 1);
        assert_eq!(flows_a[0].id, a_flow.id);

        let flows_b = registry.list_flows(Some("proj-b")).unwrap();
        assert_eq!(flows_b.len(), 1);
        assert_eq!(flows_b[0].id, b_flow.id);

        let all_graphs = registry.list_graphs(None).unwrap();
        assert!(all_graphs.len() >= 2);
        let all_flows = registry.list_flows(None).unwrap();
        assert!(all_flows.len() >= 2);
    }

    #[test]
    fn project_runtime_set_rejects_invalid_env_pairs() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let res = registry.project_runtime_set(
            "proj",
            "opencode",
            "opencode",
            None,
            &[],
            &["NO_EQUALS".to_string()],
            1000,
            1,
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_env");
    }

    #[test]
    fn attach_repo_duplicate_path_includes_recovery_hint() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let repo_path = repo_dir.to_string_lossy().to_string();
        registry
            .attach_repo("proj", &repo_path, Some("main"), RepoAccessMode::ReadWrite)
            .unwrap();

        let err = registry
            .attach_repo("proj", &repo_path, Some("main"), RepoAccessMode::ReadWrite)
            .unwrap_err();

        assert_eq!(err.code, "repo_already_attached");
        assert!(err
            .recovery_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("detach-repo")));
    }

    #[test]
    fn tick_flow_rejects_non_running_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false, None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "flow_not_running");
    }

    #[test]
    fn tick_flow_requires_runtime_configuration() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false, None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "runtime_not_configured");
    }

    #[test]
    fn project_runtime_set_rejects_unsupported_runtime_adapter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let res = registry.project_runtime_set(
            "proj",
            "not-a-real-adapter",
            "opencode",
            None,
            &[],
            &[],
            1000,
            1,
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_runtime_adapter");
    }

    #[test]
    fn tick_flow_errors_when_project_has_no_repo() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set("proj", "opencode", "opencode", None, &[], &[], 1000, 1)
            .unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false, None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "project_has_no_repo");
    }

    #[test]
    fn tick_flow_executes_ready_task_and_emits_runtime_events() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let repo_path = repo_dir.to_string_lossy().to_string();
        registry
            .attach_repo("proj", &repo_path, None, RepoAccessMode::ReadWrite)
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo '$ cargo test'; echo 'Tool: grep'; echo '- [ ] collect logs'; echo '- [x] collect logs'; echo 'I will verify outputs'; echo unit_stderr 1>&2; printf data > hm_unit.txt"
                        .to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        let err = registry
            .tick_flow(&flow.id.to_string(), false, None)
            .unwrap_err();
        assert_eq!(err.code, "checkpoints_incomplete");

        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeStarted { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeOutputChunk { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeFilesystemObserved { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeCommandObserved { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeToolCallObserved { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeTodoSnapshotUpdated { .. })));
        assert!(events.iter().any(|e| matches!(
            e.payload,
            EventPayload::RuntimeNarrativeOutputObserved { .. }
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeExited { .. })));
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeErrorClassified {
                    code,
                    category,
                    retryable,
                    ..
                } if code == "checkpoints_incomplete" && category == "checkpoint_incomplete" && !retryable
            )
        }));
        assert!(!events.iter().any(|e| {
            matches!(e.payload, EventPayload::RuntimeRecoveryScheduled { .. })
                && e.metadata.correlation.flow_id == Some(flow.id)
        }));
    }

    #[test]
    fn tick_flow_schedules_rate_limit_recovery_and_retries_task() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry
            .create_graph("proj", "g-rate-limit", &[task.id])
            .unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo 'HTTP 429 Too Many Requests' 1>&2; exit 1".to_string(),
                ],
                &[],
                1000,
                2,
            )
            .unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, None)
            .unwrap();

        assert_eq!(
            updated.task_executions.get(&task.id).map(|exec| exec.state),
            Some(TaskExecState::Pending)
        );

        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeErrorClassified {
                    code,
                    category,
                    retryable,
                    rate_limited,
                    ..
                } if code == "runtime_nonzero_exit"
                    && category == "rate_limit"
                    && *retryable
                    && *rate_limited
            )
        }));
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeRecoveryScheduled {
                    strategy,
                    from_adapter,
                    to_adapter,
                    ..
                } if strategy == "fallback_runtime"
                    && from_adapter == "opencode"
                    && to_adapter == "kilo"
            )
        }));
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::TaskRetryRequested { task_id, .. } if *task_id == task.id
            )
        }));
    }

    #[test]
    fn tick_flow_classifies_auth_errors_from_stderr_even_with_zero_exit() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo 'Error: Incorrect API key provided: invalid_key' 1>&2; exit 0"
                        .to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g-auth", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, None)
            .unwrap();

        assert_eq!(
            updated.task_executions.get(&task.id).map(|exec| exec.state),
            Some(TaskExecState::Failed)
        );

        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeOutputChunk {
                    stream: RuntimeOutputStream::Stderr,
                    content,
                    ..
                } if content.contains("Incorrect API key")
            )
        }));
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeErrorClassified {
                    code,
                    category,
                    retryable,
                    rate_limited,
                    ..
                } if code == "runtime_auth_failed"
                    && category == "runtime_execution"
                    && !retryable
                    && !rate_limited
            )
        }));
        assert!(!events
            .iter()
            .any(|e| { matches!(&e.payload, EventPayload::RuntimeRecoveryScheduled { .. }) }));
    }

    #[test]
    fn runtime_list_includes_sprint_28_adapters() {
        let registry = test_registry();
        let list = registry.runtime_list();
        let names = list
            .iter()
            .map(|r| r.adapter_name.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(names.contains("opencode"));
        assert!(names.contains("codex"));
        assert!(names.contains("claude-code"));
        assert!(names.contains("kilo"));
    }

    #[test]
    fn tick_flow_executes_with_codex_adapter() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let repo_path = repo_dir.to_string_lossy().to_string();
        registry
            .attach_repo("proj", &repo_path, None, RepoAccessMode::ReadWrite)
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "codex",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo '$ cargo fmt --check'; echo 'Tool: rg'; echo codex_stderr 1>&2; printf codex > codex.txt"
                        .to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        let _ = registry.tick_flow(&flow.id.to_string(), false, None);
        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeStarted { adapter_name, .. } if adapter_name == "codex"
            )
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeCommandObserved { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeToolCallObserved { .. })));
    }

    #[test]
    fn tick_flow_rejects_interactive_mode() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let err = registry
            .tick_flow(&flow.id.to_string(), true, None)
            .unwrap_err();
        assert_eq!(err.code, "interactive_mode_deprecated");
    }

    #[test]
    fn tick_flow_captures_runtime_output_with_quoted_args() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo \"Runtime output test successful\"".to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        let err = registry
            .tick_flow(&flow.id.to_string(), false, None)
            .unwrap_err();
        assert_eq!(err.code, "checkpoints_incomplete");

        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeOutputChunk { content, .. }
                    if content.contains("Runtime output test successful")
            )
        }));
        assert!(events
            .iter()
            .any(|e| matches!(e.payload, EventPayload::RuntimeExited { .. })));
    }

    #[test]
    fn task_runtime_override_takes_precedence_over_project_runtime() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo project_runtime; exit 1".to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        registry
            .task_runtime_set(
                &task.id.to_string(),
                "kilo",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo task_override_runtime; printf override > override.txt".to_string(),
                ],
                &[],
                1000,
            )
            .unwrap();

        let _ = registry.tick_flow(&flow.id.to_string(), false, None);
        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeStarted { adapter_name, .. } if adapter_name == "kilo"
            )
        }));
    }

    #[test]
    fn task_run_mode_manual_prevents_automatic_tick_execution() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        registry
            .task_set_run_mode(&task.id.to_string(), RunMode::Manual)
            .unwrap();

        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "opencode",
                "/usr/bin/env",
                None,
                &[
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo should_not_run".to_string(),
                ],
                &[],
                1000,
                1,
            )
            .unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, None)
            .unwrap();
        assert_eq!(
            updated.task_executions.get(&task.id).map(|e| e.state),
            Some(TaskExecState::Ready)
        );

        let events = registry.read_events(&EventFilter::all()).unwrap();
        assert!(!events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::RuntimeStarted { .. }
                    if e.metadata.correlation.flow_id == Some(flow.id)
            )
        }));
    }

    #[test]
    fn runtime_defaults_follow_task_then_flow_then_project_then_global_precedence() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        registry
            .runtime_defaults_set(
                RuntimeRole::Worker,
                "codex",
                "/usr/bin/env",
                None,
                &[],
                &[],
                1000,
                1,
            )
            .unwrap();
        registry
            .project_runtime_set_role(
                "proj",
                RuntimeRole::Worker,
                "opencode",
                "/usr/bin/env",
                None,
                &[],
                &[],
                1000,
                1,
            )
            .unwrap();
        registry
            .flow_runtime_set(
                &flow.id.to_string(),
                RuntimeRole::Worker,
                "kilo",
                "/usr/bin/env",
                None,
                &[],
                &[],
                1000,
                1,
            )
            .unwrap();
        registry
            .task_runtime_set_role(
                &task.id.to_string(),
                RuntimeRole::Worker,
                "claude-code",
                "/usr/bin/env",
                None,
                &[],
                &[],
                1000,
            )
            .unwrap();

        let task_level = registry
            .runtime_health_with_role(None, Some(&task.id.to_string()), None, RuntimeRole::Worker)
            .unwrap();
        assert_eq!(task_level.adapter_name, "claude-code");

        registry
            .task_runtime_clear_role(&task.id.to_string(), RuntimeRole::Worker)
            .unwrap();
        let flow_level = registry
            .runtime_health_with_role(None, Some(&task.id.to_string()), None, RuntimeRole::Worker)
            .unwrap();
        assert_eq!(flow_level.adapter_name, "kilo");
    }

    #[test]
    fn flow_dependencies_auto_start_downstream_flow_when_upstream_completes() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task_a = registry.create_task("proj", "Task A", None, None).unwrap();
        let task_b = registry.create_task("proj", "Task B", None, None).unwrap();
        registry
            .task_set_run_mode(&task_b.id.to_string(), RunMode::Manual)
            .unwrap();

        let graph_a = registry.create_graph("proj", "g-a", &[task_a.id]).unwrap();
        let graph_b = registry.create_graph("proj", "g-b", &[task_b.id]).unwrap();
        let flow_a = registry.create_flow(&graph_a.id.to_string(), None).unwrap();
        let flow_b = registry.create_flow(&graph_b.id.to_string(), None).unwrap();

        registry
            .flow_add_dependency(&flow_b.id.to_string(), &flow_a.id.to_string())
            .unwrap();
        registry
            .flow_set_run_mode(&flow_b.id.to_string(), RunMode::Auto)
            .unwrap();

        let flow_a = registry.start_flow(&flow_a.id.to_string()).unwrap();
        let attempt_id = registry
            .start_task_execution(&task_a.id.to_string())
            .unwrap();
        registry
            .checkpoint_complete(&attempt_id.to_string(), "checkpoint-1", None)
            .unwrap();
        registry
            .complete_task_execution(&task_a.id.to_string())
            .unwrap();
        let _ = registry
            .tick_flow(&flow_a.id.to_string(), false, None)
            .unwrap();
        let maybe_running = registry.get_flow(&flow_a.id.to_string()).unwrap();
        if maybe_running.state == FlowState::Running {
            let _ = registry
                .tick_flow(&flow_a.id.to_string(), false, None)
                .unwrap();
        }
        let completed_a = registry.get_flow(&flow_a.id.to_string()).unwrap();
        assert_eq!(completed_a.state, FlowState::Completed);

        let downstream = registry.get_flow(&flow_b.id.to_string()).unwrap();
        assert_eq!(downstream.state, FlowState::Running);
    }

    #[test]
    fn tick_flow_runs_multiple_compatible_tasks_when_max_parallel_allows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();
        configure_failing_runtime(&registry);

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();
        let graph = registry
            .create_graph("proj", "g1", &[t1.id, t2.id])
            .unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, Some(2))
            .unwrap();

        let events = registry.read_events(&EventFilter::all()).unwrap();
        let runtime_started = events
            .iter()
            .filter(|event| {
                matches!(event.payload, EventPayload::RuntimeStarted { .. })
                    && event.metadata.correlation.flow_id == Some(flow.id)
            })
            .count();
        assert_eq!(runtime_started, 2);

        let failed = updated
            .task_executions
            .values()
            .filter(|exec| exec.state == TaskExecState::Failed)
            .count();
        assert_eq!(failed, 2);
    }

    #[test]
    fn tick_flow_serializes_hard_scope_conflicts_with_observability_events() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();
        configure_failing_runtime(&registry);

        let hard_scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src", FilePermission::Write)),
        );

        let t1 = registry
            .create_task("proj", "Task 1", None, Some(hard_scope.clone()))
            .unwrap();
        let t2 = registry
            .create_task("proj", "Task 2", None, Some(hard_scope))
            .unwrap();
        let graph = registry
            .create_graph("proj", "g-hard", &[t1.id, t2.id])
            .unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, Some(2))
            .unwrap();

        let events = registry.read_events(&EventFilter::all()).unwrap();
        let runtime_started = events
            .iter()
            .filter(|event| {
                matches!(event.payload, EventPayload::RuntimeStarted { .. })
                    && event.metadata.correlation.flow_id == Some(flow.id)
            })
            .count();
        assert_eq!(runtime_started, 1);

        let failed = updated
            .task_executions
            .values()
            .filter(|exec| exec.state == TaskExecState::Failed)
            .count();
        assert_eq!(failed, 1);

        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::ScopeConflictDetected {
                    flow_id,
                    severity,
                    action,
                    ..
                } if *flow_id == flow.id && severity == "hard_conflict" && action == "serialized"
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::TaskSchedulingDeferred { flow_id, .. } if *flow_id == flow.id
            )
        }));
    }

    #[test]
    fn parse_global_parallel_limit_defaults_to_unbounded_when_missing() {
        let parsed = Registry::parse_global_parallel_limit(None).unwrap();
        assert_eq!(parsed, u16::MAX);
    }

    #[test]
    fn parse_global_parallel_limit_accepts_positive_value() {
        let parsed = Registry::parse_global_parallel_limit(Some("3".to_string())).unwrap();
        assert_eq!(parsed, 3);
    }

    #[test]
    fn parse_global_parallel_limit_rejects_zero() {
        let err = Registry::parse_global_parallel_limit(Some("0".to_string())).unwrap_err();
        assert_eq!(err.code, "invalid_global_parallel_limit");
    }

    #[test]
    fn parse_global_parallel_limit_rejects_non_numeric() {
        let err = Registry::parse_global_parallel_limit(Some("abc".to_string())).unwrap_err();
        assert_eq!(err.code, "invalid_global_parallel_limit");
    }

    #[test]
    fn tick_flow_warns_on_soft_scope_conflicts_and_allows_parallel_dispatch() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();
        configure_failing_runtime(&registry);

        let write_scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src", FilePermission::Write)),
        );
        let read_scope = Scope::new().with_filesystem(
            FilesystemScope::new().with_rule(PathRule::new("src", FilePermission::Read)),
        );

        let t1 = registry
            .create_task("proj", "Task 1", None, Some(write_scope))
            .unwrap();
        let t2 = registry
            .create_task("proj", "Task 2", None, Some(read_scope))
            .unwrap();
        let graph = registry
            .create_graph("proj", "g-soft", &[t1.id, t2.id])
            .unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let updated = registry
            .tick_flow(&flow.id.to_string(), false, Some(2))
            .unwrap();

        let events = registry.read_events(&EventFilter::all()).unwrap();
        let runtime_started = events
            .iter()
            .filter(|event| {
                matches!(event.payload, EventPayload::RuntimeStarted { .. })
                    && event.metadata.correlation.flow_id == Some(flow.id)
            })
            .count();
        assert_eq!(runtime_started, 2);

        let failed = updated
            .task_executions
            .values()
            .filter(|exec| exec.state == TaskExecState::Failed)
            .count();
        assert_eq!(failed, 2);

        assert!(events.iter().any(|event| {
            matches!(
                &event.payload,
                EventPayload::ScopeConflictDetected {
                    flow_id,
                    severity,
                    action,
                    ..
                } if *flow_id == flow.id && severity == "soft_conflict" && action == "warn_parallel"
            )
        }));
    }

    #[test]
    fn create_and_list_tasks() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        registry.create_task("proj", "Task 1", None, None).unwrap();
        registry
            .create_task("proj", "Task 2", Some("Description"), None)
            .unwrap();

        let tasks = registry.list_tasks("proj", None).unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[test]
    fn task_lifecycle() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let task = registry.create_task("proj", "My Task", None, None).unwrap();
        assert_eq!(task.state, TaskState::Open);

        let closed = registry.close_task(&task.id.to_string(), None).unwrap();
        assert_eq!(closed.state, TaskState::Closed);
    }

    #[test]
    fn filter_tasks_by_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry
            .create_task("proj", "Open Task", None, None)
            .unwrap();
        let t2 = registry
            .create_task("proj", "Closed Task", None, None)
            .unwrap();
        registry.close_task(&t2.id.to_string(), None).unwrap();

        let open_tasks = registry.list_tasks("proj", Some(TaskState::Open)).unwrap();
        assert_eq!(open_tasks.len(), 1);
        assert_eq!(open_tasks[0].id, t1.id);

        let closed_tasks = registry
            .list_tasks("proj", Some(TaskState::Closed))
            .unwrap();
        assert_eq!(closed_tasks.len(), 1);
    }

    #[test]
    fn update_task() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let task = registry
            .create_task("proj", "Original", None, None)
            .unwrap();
        let updated = registry
            .update_task(&task.id.to_string(), Some("Updated"), Some("Desc"))
            .unwrap();

        assert_eq!(updated.title, "Updated");
        assert_eq!(updated.description, Some("Desc".to_string()));
    }

    #[test]
    fn graph_create_from_tasks_and_dependency() {
        let registry = test_registry();
        let proj = registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();

        let graph = registry
            .create_graph("proj", "g1", &[t1.id, t2.id])
            .unwrap();
        assert_eq!(graph.project_id, proj.id);
        assert_eq!(graph.tasks.len(), 2);
        assert!(graph.tasks.contains_key(&t1.id));
        assert!(graph.tasks.contains_key(&t2.id));

        let updated = registry
            .add_graph_dependency(
                &graph.id.to_string(),
                &t1.id.to_string(),
                &t2.id.to_string(),
            )
            .unwrap();
        assert!(updated
            .dependencies
            .get(&t2.id)
            .is_some_and(|deps| deps.contains(&t1.id)));

        let again = registry
            .add_graph_dependency(
                &graph.id.to_string(),
                &t1.id.to_string(),
                &t2.id.to_string(),
            )
            .unwrap();
        assert_eq!(
            again.dependencies.get(&t2.id),
            updated.dependencies.get(&t2.id)
        );
    }

    #[test]
    fn add_graph_dependency_missing_task_has_hint() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();

        let err = registry
            .add_graph_dependency(
                &graph.id.to_string(),
                &t1.id.to_string(),
                &Uuid::new_v4().to_string(),
            )
            .unwrap_err();

        assert_eq!(err.code, "task_not_in_graph");
        assert!(err
            .recovery_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("included when the graph was created")));
    }

    #[test]
    fn add_graph_dependency_locked_graph_includes_locking_flow_context() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let err = registry
            .add_graph_dependency(
                &graph.id.to_string(),
                &t1.id.to_string(),
                &t1.id.to_string(),
            )
            .unwrap_err();

        assert_eq!(err.code, "graph_immutable");
        assert!(err.message.contains(&flow.id.to_string()));
        assert_eq!(
            err.context.get("locking_flow_id").map(String::as_str),
            Some(flow.id.to_string().as_str())
        );
        assert!(err
            .recovery_hint
            .as_deref()
            .is_some_and(|hint| hint.contains("Create a new graph")));
    }

    #[test]
    fn flow_create_locks_graph_and_start_sets_ready() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();

        let graph = registry
            .create_graph("proj", "g1", &[t1.id, t2.id])
            .unwrap();
        registry
            .add_graph_dependency(
                &graph.id.to_string(),
                &t1.id.to_string(),
                &t2.id.to_string(),
            )
            .unwrap();

        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let locked = registry.get_graph(&graph.id.to_string()).unwrap();
        assert_eq!(locked.state, GraphState::Locked);

        let started = registry.start_flow(&flow.id.to_string()).unwrap();
        assert_eq!(started.state, FlowState::Running);

        let started = registry.get_flow(&flow.id.to_string()).unwrap();
        assert_eq!(
            started.task_executions.get(&t1.id).map(|e| e.state),
            Some(TaskExecState::Ready)
        );
        assert_eq!(
            started.task_executions.get(&t2.id).map(|e| e.state),
            Some(TaskExecState::Pending)
        );
    }

    #[test]
    fn flow_pause_resume_abort_semantics() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let flow = registry.start_flow(&flow.id.to_string()).unwrap();
        let flow = registry.pause_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow.state, FlowState::Paused);

        let flow2 = registry.pause_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow2.state, FlowState::Paused);

        let flow = registry.resume_flow(&flow.id.to_string()).unwrap();
        assert_eq!(flow.state, FlowState::Running);

        let flow = registry
            .abort_flow(&flow.id.to_string(), Some("stop"), true)
            .unwrap();
        assert_eq!(flow.state, FlowState::Aborted);
        let flow2 = registry
            .abort_flow(&flow.id.to_string(), None, false)
            .unwrap();
        assert_eq!(flow2.state, FlowState::Aborted);
    }

    #[test]
    fn flow_restart_requires_aborted_source_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let err = registry
            .restart_flow(&flow.id.to_string(), None, false)
            .unwrap_err();
        assert_eq!(err.code, "flow_not_aborted");
    }

    #[test]
    fn flow_restart_creates_new_flow_and_copies_settings() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        registry
            .flow_runtime_set(
                &flow.id.to_string(),
                RuntimeRole::Worker,
                "opencode",
                "/usr/bin/env",
                None,
                &["sh".to_string(), "-c".to_string(), "echo hi".to_string()],
                &[],
                1000,
                1,
            )
            .unwrap();

        let flow = registry.start_flow(&flow.id.to_string()).unwrap();
        let flow = registry
            .abort_flow(&flow.id.to_string(), Some("restart-test"), true)
            .unwrap();
        assert_eq!(flow.state, FlowState::Aborted);

        let restarted = registry
            .restart_flow(&flow.id.to_string(), Some("restart"), false)
            .unwrap();
        assert_ne!(restarted.id, flow.id);
        assert_eq!(restarted.graph_id, flow.graph_id);
        assert_eq!(restarted.state, FlowState::Created);
        assert_eq!(restarted.run_mode, flow.run_mode);

        let state = registry.state().unwrap();
        let copied_worker = state
            .flow_runtime_defaults
            .get(&restarted.id)
            .and_then(|defaults| defaults.worker.as_ref())
            .expect("copied worker runtime");
        assert_eq!(copied_worker.adapter_name, "opencode");
        assert_eq!(copied_worker.binary_path, "/usr/bin/env");
    }

    #[test]
    fn list_attempts_and_checkpoints_returns_attempt_progress() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                &repo_dir.to_string_lossy(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let task = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[task.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let attempt_id = registry.start_task_execution(&task.id.to_string()).unwrap();

        let attempts = registry
            .list_attempts(Some(&flow.id.to_string()), Some(&task.id.to_string()), 10)
            .unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0].attempt_id, attempt_id);
        assert!(!attempts[0].all_checkpoints_completed);

        let checkpoints = registry.list_checkpoints(&attempt_id.to_string()).unwrap();
        assert!(!checkpoints.is_empty());
        assert_eq!(checkpoints[0].checkpoint_id, "checkpoint-1");
    }

    #[test]
    fn task_abort_and_retry_affect_flow_task_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();

        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let flow = registry.abort_task(&t1.id.to_string(), Some("no"));
        assert!(flow.is_ok());
        let flow = registry.get_flow(&flow.unwrap().id.to_string()).unwrap();
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.state),
            Some(TaskExecState::Failed)
        );

        let flow = registry
            .retry_task(&t1.id.to_string(), true, RetryMode::Clean)
            .unwrap();
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.state),
            Some(TaskExecState::Pending)
        );
        assert_eq!(
            flow.task_executions.get(&t1.id).map(|e| e.attempt_count),
            Some(0)
        );
    }

    #[test]
    fn close_task_disallowed_in_active_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.close_task(&t1.id.to_string(), None);
        assert!(res.is_err());
    }

    #[test]
    fn detach_repo_disallowed_with_active_flow() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_dir.to_string_lossy().as_ref(),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.detach_repo("proj", "main");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "project_in_active_flow");
    }

    #[test]
    fn retry_limit_exceeded_requires_reset_count() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        for _ in 0..4 {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
                    attempt_id: None,
                    from: TaskExecState::Ready,
                    to: TaskExecState::Running,
                },
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
            );
            registry.store.append(event).unwrap();
        }

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                attempt_id: None,
                from: TaskExecState::Running,
                to: TaskExecState::Failed,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        assert!(registry
            .retry_task(&t1.id.to_string(), false, RetryMode::Clean)
            .is_err());
        assert!(registry
            .retry_task(&t1.id.to_string(), true, RetryMode::Clean)
            .is_ok());
    }

    #[test]
    fn error_occurred_emitted_on_close_task_in_active_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.close_task(&t1.id.to_string(), None);
        assert!(res.is_err());

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(&e.payload, EventPayload::ErrorOccurred { error } if error.code == "task_in_active_flow")
        }));
    }

    #[test]
    fn error_occurred_emitted_on_detach_repo_with_active_flow() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        let project = registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_dir.to_string_lossy().as_ref(),
                Some("main"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.detach_repo("proj", "main");
        assert!(res.is_err());

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(&e.payload, EventPayload::ErrorOccurred { error } if error.code == "project_in_active_flow")
                && e.metadata.correlation.project_id == Some(project.id)
        }));
    }

    #[test]
    fn error_occurred_emitted_on_runtime_set_invalid_env() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let res = registry.project_runtime_set(
            "proj",
            "opencode",
            "opencode",
            None,
            &[],
            &["=VALUE".to_string()],
            1000,
            1,
        );
        assert!(res.is_err());

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(&e.payload, EventPayload::ErrorOccurred { error } if error.code == "invalid_env")
        }));
    }

    #[test]
    fn error_occurred_emitted_on_attach_repo_missing_path() {
        let registry = test_registry();
        let project = registry.create_project("proj", None).unwrap();

        let res = registry.attach_repo(
            "proj",
            "/path/does/not/exist",
            None,
            RepoAccessMode::ReadWrite,
        );
        assert!(res.is_err());

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(&e.payload, EventPayload::ErrorOccurred { error } if error.code == "repo_path_not_found")
                && e.metadata.correlation.project_id == Some(project.id)
        }));
    }

    #[test]
    fn error_occurred_not_emitted_for_read_only_get_task_failure() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();

        let before = registry.store.read_all().unwrap().len();
        let _ = registry
            .get_task("00000000-0000-0000-0000-000000000000")
            .err();
        let after = registry.store.read_all().unwrap().len();
        assert_eq!(before, after);
    }

    fn setup_flow_with_verifying_task(registry: &Registry) -> (TaskFlow, Uuid) {
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                attempt_id: None,
                from: TaskExecState::Ready,
                to: TaskExecState::Running,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        let event = Event::new(
            EventPayload::TaskExecutionStateChanged {
                flow_id: flow.id,
                task_id: t1.id,
                attempt_id: None,
                from: TaskExecState::Running,
                to: TaskExecState::Verifying,
            },
            CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
        );
        registry.store.append(event).unwrap();

        let flow = registry.get_flow(&flow.id.to_string()).unwrap();
        (flow, t1.id)
    }

    #[test]
    fn verify_override_pass_transitions_to_success() {
        let registry = test_registry();
        let (_flow, t1_id) = setup_flow_with_verifying_task(&registry);

        let updated = registry
            .verify_override(&t1_id.to_string(), "pass", "looks good")
            .unwrap();
        assert_eq!(
            updated.task_executions.get(&t1_id).map(|e| e.state),
            Some(TaskExecState::Success)
        );

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::TaskExecutionFrozen { task_id, .. } if *task_id == t1_id
            )
        }));
    }

    #[test]
    fn verify_override_fail_transitions_to_failed() {
        let registry = test_registry();
        let (_, t1_id) = setup_flow_with_verifying_task(&registry);

        let updated = registry
            .verify_override(&t1_id.to_string(), "fail", "bad output")
            .unwrap();
        assert_eq!(
            updated.task_executions.get(&t1_id).map(|e| e.state),
            Some(TaskExecState::Failed)
        );
    }

    #[test]
    fn verify_override_rejects_non_verifying_task() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.verify_override(&t1.id.to_string(), "pass", "reason");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "task_not_overridable");
    }

    #[test]
    fn verify_override_rejects_empty_reason() {
        let registry = test_registry();
        let (_, t1_id) = setup_flow_with_verifying_task(&registry);

        let res = registry.verify_override(&t1_id.to_string(), "pass", "   ");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_reason");
    }

    #[test]
    fn verify_override_rejects_invalid_decision() {
        let registry = test_registry();
        let (_, t1_id) = setup_flow_with_verifying_task(&registry);

        let res = registry.verify_override(&t1_id.to_string(), "maybe", "reason");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_decision");
    }

    fn setup_completed_flow_with_repo(registry: &Registry) -> (tempfile::TempDir, TaskFlow) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_dir.to_string_lossy().as_ref(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let exec_branch = format!("exec/{}/{t1_id}", flow.id, t1_id = t1.id);
        let out = Command::new("git")
            .args(["branch", "-f", &exec_branch, "main"])
            .current_dir(&repo_dir)
            .output()
            .expect("create exec branch");
        assert!(
            out.status.success(),
            "git branch: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        for (from, to) in [
            (TaskExecState::Ready, TaskExecState::Running),
            (TaskExecState::Running, TaskExecState::Verifying),
            (TaskExecState::Verifying, TaskExecState::Success),
        ] {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
                    attempt_id: None,
                    from,
                    to,
                },
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
            );
            registry.store.append(event).unwrap();
        }

        let event = Event::new(
            EventPayload::TaskFlowCompleted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        registry.store.append(event).unwrap();

        (tmp, registry.get_flow(&flow.id.to_string()).unwrap())
    }

    fn setup_completed_flow_with_two_repos(
        registry: &Registry,
    ) -> (tempfile::TempDir, PathBuf, PathBuf, TaskFlow, Uuid) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_a = tmp.path().join("repo-a");
        let repo_b = tmp.path().join("repo-b");
        init_git_repo(&repo_a);
        init_git_repo(&repo_b);

        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_a.to_string_lossy().as_ref(),
                Some("repo-a"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();
        registry
            .attach_repo(
                "proj",
                repo_b.to_string_lossy().as_ref(),
                Some("repo-b"),
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let exec_branch = format!("exec/{}/{task_id}", flow.id, task_id = t1.id);
        for repo in [&repo_a, &repo_b] {
            let out = Command::new("git")
                .args(["branch", "-f", &exec_branch, "main"])
                .current_dir(repo)
                .output()
                .expect("create exec branch");
            assert!(
                out.status.success(),
                "git branch: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        for (from, to) in [
            (TaskExecState::Ready, TaskExecState::Running),
            (TaskExecState::Running, TaskExecState::Verifying),
            (TaskExecState::Verifying, TaskExecState::Success),
        ] {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
                    attempt_id: None,
                    from,
                    to,
                },
                CorrelationIds::for_graph_flow_task(flow.project_id, flow.graph_id, flow.id, t1.id),
            );
            registry.store.append(event).unwrap();
        }

        let event = Event::new(
            EventPayload::TaskFlowCompleted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        registry.store.append(event).unwrap();

        (
            tmp,
            repo_a,
            repo_b,
            registry.get_flow(&flow.id.to_string()).unwrap(),
            t1.id,
        )
    }

    #[test]
    fn merge_lifecycle_prepare_approve_execute() {
        let registry = test_registry();
        let (_tmp, flow) = setup_completed_flow_with_repo(&registry);

        let ms = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Prepared);
        assert_eq!(ms.target_branch, Some("main".to_string()));

        let frozen = registry.get_flow(&flow.id.to_string()).unwrap();
        assert_eq!(frozen.state, FlowState::FrozenForMerge);

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::FlowFrozenForMerge { flow_id } if *flow_id == flow.id
            )
        }));
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::FlowIntegrationLockAcquired { flow_id, operation }
                    if *flow_id == flow.id && operation == "merge_prepare"
            )
        }));

        let ms = registry.merge_approve(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Approved);

        let ms = registry.merge_execute(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Completed);

        let merged = registry.get_flow(&flow.id.to_string()).unwrap();
        assert_eq!(merged.state, FlowState::Merged);

        let events = registry.store.read_all().unwrap();
        assert!(events.iter().any(|e| {
            matches!(
                &e.payload,
                EventPayload::FlowIntegrationLockAcquired { flow_id, operation }
                    if *flow_id == flow.id && operation == "merge_execute"
            )
        }));
    }

    #[test]
    fn merge_prepare_idempotent() {
        let registry = test_registry();
        let (_tmp, flow) = setup_completed_flow_with_repo(&registry);

        let ms1 = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        let ms2 = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        assert_eq!(ms1.status, ms2.status);
    }

    #[test]
    fn merge_approve_idempotent() {
        let registry = test_registry();
        let (_tmp, flow) = setup_completed_flow_with_repo(&registry);

        registry.merge_prepare(&flow.id.to_string(), None).unwrap();
        let ms1 = registry.merge_approve(&flow.id.to_string()).unwrap();
        let ms2 = registry.merge_approve(&flow.id.to_string()).unwrap();
        assert_eq!(ms1.status, ms2.status);
    }

    #[test]
    fn merge_prepare_rejects_non_completed_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let res = registry.merge_prepare(&flow.id.to_string(), None);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "flow_not_completed");
    }

    #[test]
    fn merge_prepare_supports_multi_repo_projects() {
        let registry = test_registry();
        let (_tmp, repo_a, repo_b, flow, _task_id) = setup_completed_flow_with_two_repos(&registry);

        let ms = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Prepared);
        assert!(ms.conflicts.is_empty(), "conflicts: {:?}", ms.conflicts);

        let merge_ref = format!("refs/heads/integration/{}/prepare", flow.id);
        for repo in [&repo_a, &repo_b] {
            let status = Command::new("git")
                .current_dir(repo)
                .args(["show-ref", "--verify", "--quiet", &merge_ref])
                .status()
                .expect("show-ref");
            assert!(
                status.success(),
                "merge branch missing in {}",
                repo.display()
            );
        }
    }

    #[test]
    fn merge_execute_is_all_or_nothing_across_repos() {
        let registry = test_registry();
        let (_tmp, repo_a, repo_b, flow, _task_id) = setup_completed_flow_with_two_repos(&registry);

        registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        registry.merge_approve(&flow.id.to_string()).unwrap();

        let head_a_before = String::from_utf8_lossy(
            &Command::new("git")
                .current_dir(&repo_a)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse a")
                .stdout,
        )
        .trim()
        .to_string();

        let prepare_branch = format!("integration/{}/prepare", flow.id);
        let prepare_worktree = WorktreeManager::new(repo_b.clone(), WorktreeConfig::default())
            .unwrap()
            .config()
            .base_dir
            .join(flow.id.to_string())
            .join("_integration_prepare");
        let _ = Command::new("git")
            .current_dir(&repo_b)
            .args([
                "worktree",
                "remove",
                "--force",
                prepare_worktree.to_str().unwrap_or(""),
            ])
            .output()
            .expect("remove prepare worktree");
        let _ = Command::new("git")
            .current_dir(&repo_b)
            .args(["branch", "-D", &prepare_branch])
            .output()
            .expect("delete prepare branch in repo-b");

        let err = registry.merge_execute(&flow.id.to_string()).unwrap_err();
        assert_eq!(err.code, "merge_branch_not_found");

        let head_a_after = String::from_utf8_lossy(
            &Command::new("git")
                .current_dir(&repo_a)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("rev-parse a")
                .stdout,
        )
        .trim()
        .to_string();
        assert_eq!(
            head_a_before, head_a_after,
            "repo-a must not merge on partial failure"
        );
    }

    #[test]
    fn merge_execute_rejects_unapproved() {
        let registry = test_registry();
        let (_tmp, flow) = setup_completed_flow_with_repo(&registry);

        registry.merge_prepare(&flow.id.to_string(), None).unwrap();
        let res = registry.merge_execute(&flow.id.to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "merge_not_approved");
    }

    #[test]
    fn merge_execute_rejects_unprepared() {
        let registry = test_registry();
        let (_tmp, flow) = setup_completed_flow_with_repo(&registry);

        let res = registry.merge_execute(&flow.id.to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "merge_not_prepared");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn merge_prepare_execute_merges_exec_branches_into_target() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo_dir = tmp.path().join("repo");
        init_git_repo(&repo_dir);

        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        registry
            .attach_repo(
                "proj",
                repo_dir.to_string_lossy().as_ref(),
                None,
                RepoAccessMode::ReadWrite,
            )
            .unwrap();

        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let t2 = registry.create_task("proj", "Task 2", None, None).unwrap();
        let graph = registry
            .create_graph("proj", "g1", &[t1.id, t2.id])
            .unwrap();
        registry
            .add_graph_dependency(
                &graph.id.to_string(),
                t1.id.to_string().as_str(),
                t2.id.to_string().as_str(),
            )
            .unwrap();

        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let manager = WorktreeManager::new(repo_dir, WorktreeConfig::default()).unwrap();
        let wt1_path = manager.path_for(flow.id, t1.id);
        if !wt1_path.exists() {
            let _ = manager.create(flow.id, t1.id, Some("HEAD")).unwrap();
        }

        let wt2_path = manager.path_for(flow.id, t2.id);
        if !wt2_path.exists() {
            let _ = manager.create(flow.id, t2.id, Some("HEAD")).unwrap();
        }

        std::fs::write(wt1_path.join("t1.txt"), "t1\n").unwrap();
        let out = Command::new("git")
            .current_dir(&wt1_path)
            .args(["add", "-A"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .current_dir(&wt1_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                "t1",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());

        let out = Command::new("git")
            .current_dir(&wt2_path)
            .args(["merge", "--ff-only", &format!("exec/{}/{}", flow.id, t1.id)])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git merge --ff-only: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        std::fs::write(wt2_path.join("t2.txt"), "t2\n").unwrap();
        let out = Command::new("git")
            .current_dir(&wt2_path)
            .args(["add", "-A"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .current_dir(&wt2_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                "t2",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());

        for task_id in [t1.id, t2.id] {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id,
                    attempt_id: None,
                    from: TaskExecState::Verifying,
                    to: TaskExecState::Success,
                },
                CorrelationIds::for_graph_flow_task(
                    flow.project_id,
                    flow.graph_id,
                    flow.id,
                    task_id,
                ),
            );
            registry.store.append(event).unwrap();
        }
        let event = Event::new(
            EventPayload::TaskFlowCompleted { flow_id: flow.id },
            CorrelationIds::for_graph_flow(flow.project_id, flow.graph_id, flow.id),
        );
        registry.store.append(event).unwrap();

        let ms = registry.merge_prepare(&flow.id.to_string(), None).unwrap();
        assert!(ms.conflicts.is_empty(), "conflicts: {:?}", ms.conflicts);

        let events = registry.store.read_all().unwrap();
        let integrated = events
            .iter()
            .filter(|e| {
                matches!(
                    &e.payload,
                    EventPayload::TaskIntegratedIntoFlow { flow_id, .. } if *flow_id == flow.id
                )
            })
            .count();
        assert_eq!(integrated, 2);

        registry.merge_approve(&flow.id.to_string()).unwrap();
        let ms = registry.merge_execute(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Completed);
        assert!(!ms.commits.is_empty());

        let merge_path = manager
            .config()
            .base_dir
            .join(flow.id.to_string())
            .join("_merge");
        assert!(!merge_path.exists());
    }

    #[test]
    fn replay_flow_reconstructs_state() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let replayed = registry.replay_flow(&flow.id.to_string()).unwrap();
        assert_eq!(replayed.id, flow.id);
        assert_eq!(replayed.state, FlowState::Running);
        assert!(replayed.task_executions.contains_key(&t1.id));
    }

    #[test]
    fn replay_flow_not_found() {
        let registry = test_registry();
        let res = registry.replay_flow(&Uuid::new_v4().to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "flow_not_found");
    }

    #[test]
    fn read_events_with_flow_filter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let _ = registry.start_flow(&flow.id.to_string()).unwrap();

        let filter = EventFilter {
            flow_id: Some(flow.id),
            ..EventFilter::default()
        };
        let events = registry.read_events(&filter).unwrap();
        assert!(!events.is_empty());
        for ev in &events {
            assert_eq!(ev.metadata.correlation.flow_id, Some(flow.id));
        }
    }

    #[test]
    fn read_events_with_task_filter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();

        let filter = EventFilter {
            task_id: Some(t1.id),
            ..EventFilter::default()
        };
        let events = registry.read_events(&filter).unwrap();
        assert_eq!(events.len(), 1);
    }
}
