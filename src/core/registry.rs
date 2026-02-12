//! Project registry for managing projects via events.
//!
//! The registry derives project state from events and provides
//! operations that emit new events.

use crate::core::diff::{unified_diff, Baseline, ChangeType, Diff, FileChange};
use crate::core::enforcement::{ScopeEnforcer, VerificationResult};
use crate::core::error::{ErrorCategory, HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload, RuntimeOutputStream};
use crate::core::flow::{FlowState, RetryMode, TaskExecState, TaskFlow};
use crate::core::graph::{GraphState, GraphTask, RetryPolicy, SuccessCriteria, TaskGraph};
use crate::core::scope::{RepoAccessMode, Scope};
use crate::core::state::{AppState, AttemptState, Project, Task, TaskState};
use crate::core::worktree::{WorktreeConfig, WorktreeError, WorktreeManager, WorktreeStatus};
use crate::storage::event_store::{EventFilter, EventStore, IndexedEventStore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::adapters::opencode::{OpenCodeAdapter, OpenCodeConfig};
use crate::adapters::runtime::{AttemptSummary, ExecutionInput, RuntimeAdapter};

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
struct DiffArtifact {
    diff: Diff,
    unified: String,
}

struct CompletionArtifacts<'a> {
    baseline_id: Uuid,
    artifact: &'a DiffArtifact,
    checkpoint_commit_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GraphValidationResult {
    pub graph_id: Uuid,
    pub valid: bool,
    pub issues: Vec<String>,
}

impl Registry {
    fn create_checkpoint_commit(worktree_path: &Path, attempt_id: Uuid) -> Option<String> {
        let status = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["status", "--porcelain"])
            .output()
            .ok()?;
        if !status.status.success() {
            return None;
        }
        if String::from_utf8_lossy(&status.stdout).trim().is_empty() {
            return None;
        }

        let add = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["add", "-A"])
            .output()
            .ok()?;
        if !add.status.success() {
            return None;
        }

        let message = format!("hivemind checkpoint {attempt_id}");
        let commit = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit",
                "-m",
                &message,
            ])
            .output()
            .ok()?;
        if !commit.status.success() {
            return None;
        }

        let head = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()?;
        if !head.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&head.stdout).trim().to_string())
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

        let verification = if let Some(scope) = &task.scope {
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

            if let Ok(manager) = Self::worktree_manager_for_flow(&flow, &state) {
                if manager.config().cleanup_on_success {
                    if let Ok(status) = manager.inspect(flow.id, task_id) {
                        if status.is_worktree {
                            let _ = manager.remove(&status.path);
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
                let _ = self.store.append(event);
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

    fn worktree_manager_for_flow(flow: &TaskFlow, state: &AppState) -> Result<WorktreeManager> {
        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:worktree_manager_for_flow",
            )
        })?;

        if project.repositories.is_empty() {
            return Err(HivemindError::user(
                "project_has_no_repo",
                "Project has no repository attached",
                "registry:worktree_manager_for_flow",
            )
            .with_hint("Attach a repo via 'hivemind project attach-repo <project> <path>'"));
        }

        if project.repositories.len() != 1 {
            return Err(HivemindError::user(
                "multiple_repos_unsupported",
                "Worktree commands currently support single-repo projects",
                "registry:worktree_manager_for_flow",
            )
            .with_hint("Detach extra repos or wait for multi-repo worktree support"));
        }

        let repo_path = PathBuf::from(&project.repositories[0].path);
        WorktreeManager::new(repo_path, WorktreeConfig::default())
            .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_manager_for_flow"))
    }

    fn ensure_task_worktree(
        flow: &TaskFlow,
        state: &AppState,
        task_id: Uuid,
        origin: &'static str,
    ) -> Result<WorktreeStatus> {
        let manager = Self::worktree_manager_for_flow(flow, state)?;
        let status = manager
            .inspect(flow.id, task_id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, origin))?;
        if status.is_worktree {
            return Ok(status);
        }

        let base = flow.base_revision.as_deref().unwrap_or("HEAD");
        manager
            .create(flow.id, task_id, Some(base))
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
    ) -> Result<Project> {
        let project = self
            .get_project(id_or_name)
            .inspect_err(|err| self.record_error_event(err, CorrelationIds::none()))?;

        let mut env_map = HashMap::new();
        for pair in env {
            let Some((k, v)) = pair.split_once('=') else {
                let err = HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. Expected KEY=VALUE"),
                    "registry:project_runtime_set",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            };
            if k.trim().is_empty() {
                let err = HivemindError::user(
                    "invalid_env",
                    format!("Invalid env var '{pair}'. KEY cannot be empty"),
                    "registry:project_runtime_set",
                );
                self.record_error_event(&err, CorrelationIds::for_project(project.id));
                return Err(err);
            }
            env_map.insert(k.to_string(), v.to_string());
        }

        let desired = crate::core::state::ProjectRuntimeConfig {
            adapter_name: adapter.to_string(),
            binary_path: binary_path.to_string(),
            model: model.clone(),
            args: args.to_vec(),
            env: env_map.clone(),
            timeout_ms,
        };
        if project.runtime.as_ref() == Some(&desired) {
            return Ok(project);
        }

        let event = Event::new(
            EventPayload::ProjectRuntimeConfigured {
                project_id: project.id,
                adapter_name: adapter.to_string(),
                binary_path: binary_path.to_string(),
                model,
                args: args.to_vec(),
                env: env_map,
                timeout_ms,
            },
            CorrelationIds::for_project(project.id),
        );

        self.store.append(event).map_err(|e| {
            HivemindError::system(
                "event_append_failed",
                e.to_string(),
                "registry:project_runtime_set",
            )
        })?;

        self.get_project(&project.id.to_string())
    }

    #[allow(clippy::too_many_lines)]
    pub fn tick_flow(&self, flow_id: &str, interactive: bool) -> Result<TaskFlow> {
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

        let task_to_run = retrying.first().copied().or_else(|| ready.first().copied());

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
                return self.get_flow(flow_id);
            }

            return Ok(flow);
        };

        let project = state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:tick_flow",
            )
        })?;

        let runtime = project.runtime.clone().ok_or_else(|| {
            HivemindError::new(
                ErrorCategory::Runtime,
                "runtime_not_configured",
                "Project has no runtime configured",
                "registry:tick_flow",
            )
        })?;

        if runtime.adapter_name != "opencode" {
            return Err(HivemindError::user(
                "unsupported_runtime",
                format!("Unsupported runtime adapter '{}'", runtime.adapter_name),
                "registry:tick_flow",
            ));
        }

        let worktree_status =
            Self::ensure_task_worktree(&flow, &state, task_id, "registry:tick_flow")?;

        let exec = flow.task_executions.get(&task_id).ok_or_else(|| {
            HivemindError::system(
                "task_exec_not_found",
                "Task execution not found",
                "registry:tick_flow",
            )
        })?;

        if exec.state == TaskExecState::Retry && exec.retry_mode == RetryMode::Clean {
            let base = flow.base_revision.as_deref().unwrap_or("HEAD");
            let branch = format!("exec/{}/{task_id}", flow.id);
            Self::checkout_and_clean_worktree(
                &worktree_status.path,
                &branch,
                base,
                "registry:tick_flow",
            )?;
        }

        let next_attempt_number = exec.attempt_count.saturating_add(1);

        // Ensure this worktree contains the latest changes from dependency tasks.
        // Each task runs in its own worktree/branch (`exec/<flow>/<task>`), so dependent
        // tasks must merge dependency branch heads to see upstream work.
        if let Some(deps) = graph.dependencies.get(&task_id) {
            let mut dep_ids: Vec<Uuid> = deps.iter().copied().collect();
            dep_ids.sort();

            for dep_task_id in dep_ids {
                let dep_branch = format!("exec/{}/{dep_task_id}", flow.id);
                let dep_ref = format!("refs/heads/{dep_branch}");

                let ref_exists = std::process::Command::new("git")
                    .current_dir(&worktree_status.path)
                    .args(["show-ref", "--verify", "--quiet", &dep_ref])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if !ref_exists {
                    continue;
                }

                let already_contains = std::process::Command::new("git")
                    .current_dir(&worktree_status.path)
                    .args(["merge-base", "--is-ancestor", &dep_branch, "HEAD"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);

                if already_contains {
                    continue;
                }

                let merge = std::process::Command::new("git")
                    .current_dir(&worktree_status.path)
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
                        .current_dir(&worktree_status.path)
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

            (Some(ctx), priors)
        } else {
            (None, Vec::new())
        };

        self.store
            .append(Event::new(
                EventPayload::RuntimeStarted {
                    adapter_name: runtime.adapter_name.clone(),
                    task_id,
                    attempt_id,
                },
                attempt_corr.clone(),
            ))
            .map_err(|e| {
                HivemindError::system("event_append_failed", e.to_string(), "registry:tick_flow")
            })?;

        let timeout = Duration::from_millis(runtime.timeout_ms);
        let mut cfg = OpenCodeConfig::new(PathBuf::from(runtime.binary_path));
        cfg.model = runtime.model.clone().or(cfg.model);
        cfg.base.args = runtime.args;
        cfg.base.env = runtime.env;

        let target_dir = self
            .config
            .data_dir
            .join("cargo-target")
            .join(flow.id.to_string())
            .join(task_id.to_string())
            .join(attempt_id.to_string());
        let _ = fs::create_dir_all(&target_dir);
        cfg.base
            .env
            .entry("CARGO_TARGET_DIR".to_string())
            .or_insert_with(|| target_dir.to_string_lossy().to_string());

        cfg.base.timeout = timeout;

        let mut adapter = OpenCodeAdapter::new(cfg);
        if let Err(e) = adapter.initialize() {
            let reason = format!("{}: {}", e.code, e.message);
            self.store
                .append(Event::new(
                    EventPayload::RuntimeTerminated { attempt_id, reason },
                    attempt_corr,
                ))
                .map_err(|err| {
                    HivemindError::system(
                        "event_append_failed",
                        err.to_string(),
                        "registry:tick_flow",
                    )
                })?;
            let _ = self.complete_task_execution(&task_id.to_string());
            return self.get_flow(flow_id);
        }
        if let Err(e) = adapter.prepare(task_id, &worktree_status.path) {
            let reason = format!("{}: {}", e.code, e.message);
            self.store
                .append(Event::new(
                    EventPayload::RuntimeTerminated { attempt_id, reason },
                    attempt_corr,
                ))
                .map_err(|err| {
                    HivemindError::system(
                        "event_append_failed",
                        err.to_string(),
                        "registry:tick_flow",
                    )
                })?;
            let _ = self.complete_task_execution(&task_id.to_string());
            return self.get_flow(flow_id);
        }

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

        let (report, terminated_reason) = if interactive {
            let mut stdout = std::io::stdout();

            let res = adapter.execute_interactive(&input, |evt| {
                match evt {
                    crate::adapters::opencode::InteractiveAdapterEvent::Output { content } => {
                        let _ = stdout.write_all(content.as_bytes());
                        let _ = stdout.flush();
                        let event = Event::new(
                            EventPayload::RuntimeOutputChunk {
                                attempt_id,
                                stream: RuntimeOutputStream::Stdout,
                                content,
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                    crate::adapters::opencode::InteractiveAdapterEvent::Input { content } => {
                        let event = Event::new(
                            EventPayload::RuntimeInputProvided {
                                attempt_id,
                                content,
                            },
                            attempt_corr.clone(),
                        );
                        self.store.append(event).map_err(|e| e.to_string())?;
                    }
                    crate::adapters::opencode::InteractiveAdapterEvent::Interrupted => {
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
                    let reason = format!("{}: {}", e.code, e.message);
                    self.store
                        .append(Event::new(
                            EventPayload::RuntimeTerminated { attempt_id, reason },
                            attempt_corr,
                        ))
                        .map_err(|err| {
                            HivemindError::system(
                                "event_append_failed",
                                err.to_string(),
                                "registry:tick_flow",
                            )
                        })?;
                    let _ = self.complete_task_execution(&task_id.to_string());
                    return self.get_flow(flow_id);
                }
            }
        } else {
            let report = match adapter.execute(input) {
                Ok(r) => r,
                Err(e) => {
                    let reason = format!("{}: {}", e.code, e.message);
                    self.store
                        .append(Event::new(
                            EventPayload::RuntimeTerminated { attempt_id, reason },
                            attempt_corr,
                        ))
                        .map_err(|err| {
                            HivemindError::system(
                                "event_append_failed",
                                err.to_string(),
                                "registry:tick_flow",
                            )
                        })?;
                    let _ = self.complete_task_execution(&task_id.to_string());
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
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stdout,
                        content: chunk.to_string(),
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
            }
            for chunk in report.stderr.lines() {
                let event = Event::new(
                    EventPayload::RuntimeOutputChunk {
                        attempt_id,
                        stream: RuntimeOutputStream::Stderr,
                        content: chunk.to_string(),
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
            }
        }

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

        self.complete_task_execution(&task_id.to_string())?;
        self.process_verifying_task(flow_id, task_id)
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
                && !matches!(f.state, FlowState::Completed | FlowState::Aborted)
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
            let err = HivemindError::user(
                "graph_immutable",
                format!("Graph '{graph_id}' is immutable"),
                "registry:add_graph_dependency",
            );
            self.record_error_event(&err, CorrelationIds::for_graph(graph.project_id, gid));
            return Err(err);
        }

        if !graph.tasks.contains_key(&from) || !graph.tasks.contains_key(&to) {
            let err = HivemindError::user(
                "task_not_in_graph",
                "One or more tasks are not in the graph",
                "registry:add_graph_dependency",
            );
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
            );
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
            f.graph_id == graph.id && !matches!(f.state, FlowState::Completed | FlowState::Aborted)
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
                return self.get_flow(flow_id);
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
            if project.repositories.len() == 1 {
                if let (Some(base), Some(repo)) =
                    (base_revision.as_deref(), project.repositories.first())
                {
                    let _ = std::process::Command::new("git")
                        .current_dir(&repo.path)
                        .args(["branch", "-f", &format!("flow/{}", flow.id), base])
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

        self.get_flow(flow_id)
    }

    pub fn worktree_list(&self, flow_id: &str) -> Result<Vec<WorktreeStatus>> {
        let flow = self.get_flow(flow_id)?;
        let state = self.state()?;
        let manager = Self::worktree_manager_for_flow(&flow, &state)?;

        let mut statuses = Vec::new();
        for task_id in flow.task_executions.keys() {
            let status = manager
                .inspect(flow.id, *task_id)
                .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_list"))?;
            statuses.push(status);
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

    pub fn worktree_cleanup(&self, flow_id: &str) -> Result<()> {
        let flow = self.get_flow(flow_id)?;
        let state = self.state()?;
        let manager = Self::worktree_manager_for_flow(&flow, &state)?;
        manager
            .cleanup_flow(flow.id)
            .map_err(|e| Self::worktree_error_to_hivemind(e, "registry:worktree_cleanup"))
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

        self.get_flow(flow_id)
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
            if let Ok(manager) = Self::worktree_manager_for_flow(&flow, &state) {
                let base = flow.base_revision.as_deref().unwrap_or("HEAD");
                let mut status = manager.inspect(flow.id, id).ok();
                if status.as_ref().is_none_or(|s| !s.is_worktree) {
                    let _ = manager.create(flow.id, id, Some(base));
                    status = manager.inspect(flow.id, id).ok();
                }

                if let Some(status) = status.filter(|s| s.is_worktree) {
                    let branch = format!("exec/{}/{id}", flow.id);
                    Self::checkout_and_clean_worktree(
                        &status.path,
                        &branch,
                        base,
                        "registry:retry_task",
                    )?;
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

        let checkpoint_commit_sha = Self::create_checkpoint_commit(&status.path, attempt.id);

        self.emit_task_execution_completion_events(
            &flow,
            id,
            &attempt,
            CompletionArtifacts {
                baseline_id,
                artifact: &artifact,
                checkpoint_commit_sha,
            },
            origin,
        )?;

        self.get_flow(&flow.id.to_string())
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

        if matches!(flow.state, FlowState::Completed | FlowState::Aborted) {
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
            if let Ok(manager) = Self::worktree_manager_for_flow(&updated, &state) {
                if manager.config().cleanup_on_success {
                    if let Ok(status) = manager.inspect(updated.id, id) {
                        if status.is_worktree {
                            let _ = manager.remove(&status.path);
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
        let flow = self.get_flow(flow_id)?;

        if flow.state != FlowState::Completed {
            return Err(HivemindError::user(
                "flow_not_completed",
                "Flow has not completed successfully",
                "registry:merge_prepare",
            ));
        }

        let state = self.state()?;
        if let Some(ms) = state.merge_states.get(&flow.id) {
            if ms.status == crate::core::state::MergeStatus::Prepared && ms.conflicts.is_empty() {
                return Ok(ms.clone());
            }
        }

        let graph = state.graphs.get(&flow.graph_id).ok_or_else(|| {
            HivemindError::system(
                "graph_not_found",
                "Graph not found",
                "registry:merge_prepare",
            )
        })?;

        let mut conflicts = Vec::new();

        if let Some(project) = state.projects.get(&flow.project_id) {
            if project.repositories.len() == 1 {
                let manager = Self::worktree_manager_for_flow(&flow, &state)?;
                let base_ref = target_branch.unwrap_or("HEAD");
                let merge_branch = format!("flow/{}", flow.id);
                let merge_path = manager
                    .config()
                    .base_dir
                    .join(flow.id.to_string())
                    .join("_merge");

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
                        conflicts.push(format!("task {task_id}: missing branch '{task_branch}'"));
                        break;
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
                            "--squash",
                            "--no-commit",
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

                        let can_auto_resolve = graph
                            .dependencies
                            .get(&task_id)
                            .is_some_and(|deps| !deps.is_empty());

                        if can_auto_resolve && !unmerged.is_empty() {
                            let mut ok = true;
                            for path in unmerged.lines().map(str::trim).filter(|s| !s.is_empty()) {
                                let checkout = std::process::Command::new("git")
                                    .current_dir(&merge_path)
                                    .args(["checkout", "--theirs", "--", path])
                                    .output();
                                if !checkout.as_ref().is_ok_and(|o| o.status.success()) {
                                    ok = false;
                                    break;
                                }

                                let add = std::process::Command::new("git")
                                    .current_dir(&merge_path)
                                    .args(["add", "--", path])
                                    .output();
                                if !add.as_ref().is_ok_and(|o| o.status.success()) {
                                    ok = false;
                                    break;
                                }
                            }

                            if ok {
                                let still_unmerged = std::process::Command::new("git")
                                    .current_dir(&merge_path)
                                    .args(["diff", "--name-only", "--diff-filter=U"])
                                    .output()
                                    .ok()
                                    .filter(|o| o.status.success())
                                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                                    .unwrap_or_default();

                                if still_unmerged.is_empty() {
                                    // Proceed with commit after auto-resolving using task branch versions.
                                } else {
                                    ok = false;
                                }
                            }

                            if ok {
                                // Continue to commit for this task.
                            } else {
                                conflicts.push(format!("task {task_id}: conflicts in: {unmerged}"));
                                let _ = std::process::Command::new("git")
                                    .current_dir(&merge_path)
                                    .args(["merge", "--abort"])
                                    .output();
                                break;
                            }
                        } else {
                            let details = if unmerged.is_empty() {
                                String::from_utf8_lossy(&merge.stderr).to_string()
                            } else {
                                format!("conflicts in: {unmerged}")
                            };
                            conflicts.push(format!("task {task_id}: {details}"));

                            let _ = std::process::Command::new("git")
                                .current_dir(&merge_path)
                                .args(["merge", "--abort"])
                                .output();
                            break;
                        }
                    }

                    let commit_msg = format!("Integrate task {task_id}");
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
                            "--allow-empty",
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
                }
            }
        }

        let event = Event::new(
            EventPayload::MergePrepared {
                flow_id: flow.id,
                target_branch: target_branch.map(String::from),
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

        let event = Event::new(
            EventPayload::MergeApproved {
                flow_id: flow.id,
                user: None,
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

        let mut commits = Vec::new();
        if let Some(project) = state.projects.get(&flow.project_id) {
            if project.repositories.len() == 1 {
                let manager = Self::worktree_manager_for_flow(&flow, &state)?;
                let repo_path = manager.repo_path();

                let dirty = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["status", "--porcelain"])
                    .output()
                    .map_err(|e| {
                        HivemindError::system(
                            "git_status_failed",
                            e.to_string(),
                            "registry:merge_execute",
                        )
                    })?;
                if !dirty.status.success() {
                    return Err(HivemindError::git(
                        "git_status_failed",
                        String::from_utf8_lossy(&dirty.stderr).to_string(),
                        "registry:merge_execute",
                    ));
                }
                let dirty_stdout = String::from_utf8_lossy(&dirty.stdout);
                let has_dirty_files = dirty_stdout
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
                        "Repository has uncommitted changes",
                        "registry:merge_execute",
                    )
                    .with_hint("Commit or stash changes before running 'hivemind merge execute'"));
                }

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

                let target = ms
                    .target_branch
                    .as_deref()
                    .unwrap_or(current_branch.as_str());
                if target == "HEAD" {
                    return Err(HivemindError::user(
                        "detached_head",
                        "Cannot merge into detached HEAD",
                        "registry:merge_execute",
                    )
                    .with_hint("Re-run with --target <branch> or checkout a branch"));
                }

                let merge_branch = format!("flow/{}", flow.id);
                let merge_ref = format!("refs/heads/{merge_branch}");
                let merge_ref_exists = std::process::Command::new("git")
                    .current_dir(repo_path)
                    .args(["show-ref", "--verify", "--quiet", &merge_ref])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !merge_ref_exists {
                    return Err(HivemindError::user(
                        "merge_branch_not_found",
                        "Prepared integration branch not found",
                        "registry:merge_execute",
                    )
                    .with_hint("Run 'hivemind merge prepare' again"));
                }

                if current_branch != target {
                    let checkout = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", target])
                        .output()
                        .map_err(|e| {
                            HivemindError::system(
                                "git_checkout_failed",
                                e.to_string(),
                                "registry:merge_execute",
                            )
                        })?;
                    if !checkout.status.success() {
                        return Err(HivemindError::git(
                            "git_checkout_failed",
                            String::from_utf8_lossy(&checkout.stderr).to_string(),
                            "registry:merge_execute",
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
                        HivemindError::system(
                            "git_merge_failed",
                            e.to_string(),
                            "registry:merge_execute",
                        )
                    })?;
                if !merge_out.status.success() {
                    return Err(HivemindError::git(
                        "git_merge_failed",
                        String::from_utf8_lossy(&merge_out.stderr).to_string(),
                        "registry:merge_execute",
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

                if current_branch != target {
                    let _ = std::process::Command::new("git")
                        .current_dir(repo_path)
                        .args(["checkout", &current_branch])
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

                if manager.config().cleanup_on_success {
                    for task_id in flow.task_executions.keys() {
                        let branch = format!("exec/{}/{task_id}", flow.id);
                        let _ = std::process::Command::new("git")
                            .current_dir(repo_path)
                            .args(["branch", "-D", &branch])
                            .output();
                    }

                    let flow_branch = format!("flow/{}", flow.id);
                    if current_branch != flow_branch {
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
    use crate::storage::event_store::InMemoryEventStore;
    use std::process::Command;

    fn test_registry() -> Registry {
        let store = Arc::new(InMemoryEventStore::new());
        let config = RegistryConfig::with_dir(PathBuf::from("/tmp/test"));
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
        );
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "invalid_env");
    }

    #[test]
    fn tick_flow_rejects_non_running_flow() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false);
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

        let res = registry.tick_flow(&flow.id.to_string(), false);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "runtime_not_configured");
    }

    #[test]
    fn tick_flow_rejects_unsupported_runtime_adapter() {
        let registry = test_registry();
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        registry
            .project_runtime_set(
                "proj",
                "not-a-real-adapter",
                "opencode",
                None,
                &[],
                &[],
                1000,
            )
            .unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "unsupported_runtime");
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
            .project_runtime_set("proj", "opencode", "opencode", None, &[], &[], 1000)
            .unwrap();

        let res = registry.tick_flow(&flow.id.to_string(), false);
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
                    "echo unit_stdout; echo unit_stderr 1>&2; printf data > hm_unit.txt"
                        .to_string(),
                ],
                &[],
                1000,
            )
            .unwrap();

        let _ = registry.tick_flow(&flow.id.to_string(), false).unwrap();

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
            .any(|e| matches!(e.payload, EventPayload::RuntimeExited { .. })));
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

    fn setup_completed_flow(registry: &Registry) -> TaskFlow {
        registry.create_project("proj", None).unwrap();
        let t1 = registry.create_task("proj", "Task 1", None, None).unwrap();
        let graph = registry.create_graph("proj", "g1", &[t1.id]).unwrap();
        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        for (from, to) in [
            (TaskExecState::Ready, TaskExecState::Running),
            (TaskExecState::Running, TaskExecState::Verifying),
            (TaskExecState::Verifying, TaskExecState::Success),
        ] {
            let event = Event::new(
                EventPayload::TaskExecutionStateChanged {
                    flow_id: flow.id,
                    task_id: t1.id,
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

        registry.get_flow(&flow.id.to_string()).unwrap()
    }

    #[test]
    fn merge_lifecycle_prepare_approve_execute() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        let ms = registry
            .merge_prepare(&flow.id.to_string(), Some("main"))
            .unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Prepared);
        assert_eq!(ms.target_branch, Some("main".to_string()));

        let ms = registry.merge_approve(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Approved);

        let ms = registry.merge_execute(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Completed);
    }

    #[test]
    fn merge_prepare_idempotent() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

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
        let flow = setup_completed_flow(&registry);

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
    fn merge_execute_rejects_unapproved() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

        registry.merge_prepare(&flow.id.to_string(), None).unwrap();
        let res = registry.merge_execute(&flow.id.to_string());
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().code, "merge_not_approved");
    }

    #[test]
    fn merge_execute_rejects_unprepared() {
        let registry = test_registry();
        let flow = setup_completed_flow(&registry);

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
                t2.id.to_string().as_str(),
                t1.id.to_string().as_str(),
            )
            .unwrap();

        let flow = registry.create_flow(&graph.id.to_string(), None).unwrap();
        let flow = registry.start_flow(&flow.id.to_string()).unwrap();

        let manager = WorktreeManager::new(repo_dir.clone(), WorktreeConfig::default()).unwrap();
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
        assert!(ms.conflicts.is_empty());

        registry.merge_approve(&flow.id.to_string()).unwrap();
        let ms = registry.merge_execute(&flow.id.to_string()).unwrap();
        assert_eq!(ms.status, crate::core::state::MergeStatus::Completed);
        assert!(!ms.commits.is_empty());

        let merge_path = repo_dir
            .join(".hivemind/worktrees")
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
