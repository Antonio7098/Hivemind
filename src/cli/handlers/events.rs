//! Events command handlers.

use crate::cli::commands::EventCommands;
use crate::cli::handlers::common::{get_registry, print_structured};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;
use chrono::{DateTime, Utc};
use uuid::Uuid;
#[allow(clippy::too_many_lines)]
fn event_type_label(payload: &crate::core::events::EventPayload) -> &'static str {
    use crate::core::events::EventPayload;
    match payload {
        EventPayload::ErrorOccurred { .. } => "error_occurred",
        EventPayload::ProjectCreated { .. } => "project_created",
        EventPayload::ProjectUpdated { .. } => "project_updated",
        EventPayload::ProjectDeleted { .. } => "project_deleted",
        EventPayload::ProjectRuntimeConfigured { .. } => "project_runtime_configured",
        EventPayload::ProjectRuntimeRoleConfigured { .. } => "project_runtime_role_configured",
        EventPayload::GlobalRuntimeConfigured { .. } => "global_runtime_configured",
        EventPayload::TaskCreated { .. } => "task_created",
        EventPayload::TaskUpdated { .. } => "task_updated",
        EventPayload::TaskRuntimeConfigured { .. } => "task_runtime_configured",
        EventPayload::TaskRuntimeRoleConfigured { .. } => "task_runtime_role_configured",
        EventPayload::TaskRuntimeCleared { .. } => "task_runtime_cleared",
        EventPayload::TaskRuntimeRoleCleared { .. } => "task_runtime_role_cleared",
        EventPayload::TaskRunModeSet { .. } => "task_run_mode_set",
        EventPayload::TaskClosed { .. } => "task_closed",
        EventPayload::TaskDeleted { .. } => "task_deleted",
        EventPayload::RepositoryAttached { .. } => "repo_attached",
        EventPayload::RepositoryDetached { .. } => "repo_detached",
        EventPayload::GovernanceProjectStorageInitialized { .. } => {
            "governance_project_storage_initialized"
        }
        EventPayload::GovernanceArtifactUpserted { .. } => "governance_artifact_upserted",
        EventPayload::GovernanceArtifactDeleted { .. } => "governance_artifact_deleted",
        EventPayload::GovernanceAttachmentLifecycleUpdated { .. } => {
            "governance_attachment_lifecycle_updated"
        }
        EventPayload::GovernanceStorageMigrated { .. } => "governance_storage_migrated",
        EventPayload::GovernanceSnapshotCreated { .. } => "governance_snapshot_created",
        EventPayload::GovernanceSnapshotRestored { .. } => "governance_snapshot_restored",
        EventPayload::GovernanceDriftDetected { .. } => "governance_drift_detected",
        EventPayload::GovernanceRepairApplied { .. } => "governance_repair_applied",
        EventPayload::GraphSnapshotStarted { .. } => "graph_snapshot_started",
        EventPayload::GraphSnapshotCompleted { .. } => "graph_snapshot_completed",
        EventPayload::GraphSnapshotFailed { .. } => "graph_snapshot_failed",
        EventPayload::GraphSnapshotDiffDetected { .. } => "graph_snapshot_diff_detected",
        EventPayload::GraphQueryExecuted { .. } => "graph_query_executed",
        EventPayload::ConstitutionInitialized { .. } => "constitution_initialized",
        EventPayload::ConstitutionUpdated { .. } => "constitution_updated",
        EventPayload::ConstitutionValidated { .. } => "constitution_validated",
        EventPayload::ConstitutionViolationDetected { .. } => "constitution_violation_detected",
        EventPayload::TemplateInstantiated { .. } => "template_instantiated",
        EventPayload::AttemptContextOverridesApplied { .. } => "attempt_context_overrides_applied",
        EventPayload::ContextWindowCreated { .. } => "context_window_created",
        EventPayload::ContextOpApplied { .. } => "context_op_applied",
        EventPayload::ContextWindowSnapshotCreated { .. } => "context_window_snapshot_created",
        EventPayload::AttemptContextAssembled { .. } => "attempt_context_assembled",
        EventPayload::AttemptContextTruncated { .. } => "attempt_context_truncated",
        EventPayload::AttemptContextDelivered { .. } => "attempt_context_delivered",
        EventPayload::TaskGraphCreated { .. } => "graph_created",
        EventPayload::TaskAddedToGraph { .. } => "graph_task_added",
        EventPayload::DependencyAdded { .. } => "graph_dependency_added",
        EventPayload::GraphTaskCheckAdded { .. } => "graph_task_check_added",
        EventPayload::ScopeAssigned { .. } => "graph_scope_assigned",
        EventPayload::TaskGraphValidated { .. } => "graph_validated",
        EventPayload::TaskGraphLocked { .. } => "graph_locked",
        EventPayload::TaskGraphDeleted { .. } => "graph_deleted",
        EventPayload::TaskFlowCreated { .. } => "flow_created",
        EventPayload::TaskFlowDependencyAdded { .. } => "flow_dependency_added",
        EventPayload::TaskFlowRunModeSet { .. } => "flow_run_mode_set",
        EventPayload::TaskFlowRuntimeConfigured { .. } => "flow_runtime_configured",
        EventPayload::TaskFlowRuntimeCleared { .. } => "flow_runtime_cleared",
        EventPayload::TaskFlowStarted { .. } => "flow_started",
        EventPayload::TaskFlowPaused { .. } => "flow_paused",
        EventPayload::TaskFlowResumed { .. } => "flow_resumed",
        EventPayload::TaskFlowCompleted { .. } => "flow_completed",
        EventPayload::TaskFlowAborted { .. } => "flow_aborted",
        EventPayload::TaskFlowDeleted { .. } => "flow_deleted",
        EventPayload::TaskReady { .. } => "task_ready",
        EventPayload::TaskBlocked { .. } => "task_blocked",
        EventPayload::ScopeConflictDetected { .. } => "scope_conflict_detected",
        EventPayload::TaskSchedulingDeferred { .. } => "task_scheduling_deferred",
        EventPayload::TaskExecutionStateChanged { .. } => "task_execution_state_changed",
        EventPayload::TaskExecutionStarted { .. } => "task_execution_started",
        EventPayload::TaskExecutionSucceeded { .. } => "task_execution_succeeded",
        EventPayload::TaskExecutionFailed { .. } => "task_execution_failed",
        EventPayload::AttemptStarted { .. } => "attempt_started",
        EventPayload::BaselineCaptured { .. } => "baseline_captured",
        EventPayload::FileModified { .. } => "file_modified",
        EventPayload::DiffComputed { .. } => "diff_computed",
        EventPayload::CheckStarted { .. } => "check_started",
        EventPayload::CheckCompleted { .. } => "check_completed",
        EventPayload::MergeCheckStarted { .. } => "merge_check_started",
        EventPayload::MergeCheckCompleted { .. } => "merge_check_completed",
        EventPayload::CheckpointDeclared { .. } => "checkpoint_declared",
        EventPayload::CheckpointActivated { .. } => "checkpoint_activated",
        EventPayload::CheckpointCompleted { .. } => "checkpoint_completed",
        EventPayload::AllCheckpointsCompleted { .. } => "all_checkpoints_completed",
        EventPayload::CheckpointCommitCreated { .. } => "checkpoint_commit_created",
        EventPayload::ScopeValidated { .. } => "scope_validated",
        EventPayload::ScopeViolationDetected { .. } => "scope_violation_detected",
        EventPayload::RetryContextAssembled { .. } => "retry_context_assembled",
        EventPayload::TaskRetryRequested { .. } => "task_retried",
        EventPayload::TaskAborted { .. } => "task_aborted",
        EventPayload::HumanOverride { .. } => "human_override",
        EventPayload::MergePrepared { .. } => "merge_prepared",
        EventPayload::TaskExecutionFrozen { .. } => "task_execution_frozen",
        EventPayload::TaskIntegratedIntoFlow { .. } => "task_integrated_into_flow",
        EventPayload::MergeConflictDetected { .. } => "merge_conflict_detected",
        EventPayload::FlowFrozenForMerge { .. } => "flow_frozen_for_merge",
        EventPayload::FlowIntegrationLockAcquired { .. } => "flow_integration_lock_acquired",
        EventPayload::MergeApproved { .. } => "merge_approved",
        EventPayload::MergeCompleted { .. } => "merge_completed",
        EventPayload::WorktreeCleanupPerformed { .. } => "worktree_cleanup_performed",
        EventPayload::RuntimeCapabilitiesEvaluated { .. } => "runtime_capabilities_evaluated",
        EventPayload::AgentInvocationStarted { .. } => "agent_invocation_started",
        EventPayload::AgentTurnStarted { .. } => "agent_turn_started",
        EventPayload::ModelRequestPrepared { .. } => "model_request_prepared",
        EventPayload::ModelResponseReceived { .. } => "model_response_received",
        EventPayload::ToolCallRequested { .. } => "tool_call_requested",
        EventPayload::ToolCallStarted { .. } => "tool_call_started",
        EventPayload::ToolCallCompleted { .. } => "tool_call_completed",
        EventPayload::ToolCallFailed { .. } => "tool_call_failed",
        EventPayload::AgentTurnCompleted { .. } => "agent_turn_completed",
        EventPayload::AgentInvocationCompleted { .. } => "agent_invocation_completed",
        EventPayload::RuntimeStarted { .. } => "runtime_started",
        EventPayload::RuntimeEnvironmentPrepared { .. } => "runtime_environment_prepared",
        EventPayload::RuntimeOutputChunk { .. } => "runtime_output_chunk",
        EventPayload::RuntimeInputProvided { .. } => "runtime_input_provided",
        EventPayload::RuntimeInterrupted { .. } => "runtime_interrupted",
        EventPayload::RuntimeExited { .. } => "runtime_exited",
        EventPayload::RuntimeTerminated { .. } => "runtime_terminated",
        EventPayload::RuntimeErrorClassified { .. } => "runtime_error_classified",
        EventPayload::RuntimeRecoveryScheduled { .. } => "runtime_recovery_scheduled",
        EventPayload::RuntimeFilesystemObserved { .. } => "runtime_filesystem_observed",
        EventPayload::RuntimeCommandObserved { .. } => "runtime_command_observed",
        EventPayload::RuntimeToolCallObserved { .. } => "runtime_tool_call_observed",
        EventPayload::RuntimeTodoSnapshotUpdated { .. } => "runtime_todo_snapshot_updated",
        EventPayload::RuntimeNarrativeOutputObserved { .. } => "runtime_narrative_output_observed",
        EventPayload::Unknown => "unknown",
    }
}

fn print_events_table(events: Vec<crate::core::events::Event>) {
    if events.is_empty() {
        println!("No events found.");
        return;
    }
    println!("{:<6}  {:<24}  {:<22}  PROJECT", "SEQ", "TYPE", "TIMESTAMP",);
    println!("{}", "-".repeat(90));
    for ev in events {
        let seq = ev.metadata.sequence.unwrap_or(0);
        let typ = event_type_label(&ev.payload);
        let ts = ev.metadata.timestamp.to_rfc3339();
        let proj = ev
            .metadata
            .correlation
            .project_id
            .map_or_else(|| "-".to_string(), |p| p.to_string());
        println!("{seq:<6}  {typ:<24}  {ts:<22}  {proj}");
    }
}

const REDACTED_SECRET: &str = "[REDACTED]";

fn redact_events_for_stream(
    events: Vec<crate::core::events::Event>,
) -> Vec<crate::core::events::Event> {
    events.into_iter().map(redact_stream_event).collect()
}

fn redact_stream_event(event: crate::core::events::Event) -> crate::core::events::Event {
    let Ok(mut raw) = serde_json::to_value(&event) else {
        return event;
    };
    redact_json_secrets(&mut raw, None);
    serde_json::from_value(raw).unwrap_or(event)
}

fn redact_json_secrets(value: &mut serde_json::Value, parent_key: Option<&str>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_json_key(key) {
                    *child = serde_json::Value::String(REDACTED_SECRET.to_string());
                    continue;
                }
                redact_json_secrets(child, Some(key));
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                redact_json_secrets(child, parent_key);
            }
        }
        serde_json::Value::String(raw) => {
            if parent_key.is_some_and(is_sensitive_json_key) {
                *raw = REDACTED_SECRET.to_string();
            } else {
                *raw = redact_inline_secret_tokens(raw);
            }
        }
        _ => {}
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    [
        "api_key",
        "authorization",
        "access_token",
        "refresh_token",
        "token",
        "secret",
        "password",
        "credential",
        "private_key",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn redact_inline_secret_tokens(raw: &str) -> String {
    let mut current = raw.to_string();
    for (prefix, min_len) in [("sk-or-v1-", 12_usize), ("sk-proj-", 12), ("sk-", 20)] {
        current = redact_prefixed_token(&current, prefix, min_len);
    }
    current
}

fn redact_prefixed_token(input: &str, prefix: &str, min_suffix_len: usize) -> String {
    if !input.contains(prefix) {
        return input.to_string();
    }

    let mut redacted = String::with_capacity(input.len());
    let mut index = 0_usize;
    while index < input.len() {
        if input[index..].starts_with(prefix) {
            let start = index;
            let mut end = start + prefix.len();
            while end < input.len() {
                let ch = input[end..].chars().next().unwrap_or_default();
                if ch.is_whitespace()
                    || matches!(
                        ch,
                        '"' | '\'' | ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>'
                    )
                {
                    break;
                }
                end += ch.len_utf8();
            }
            if end.saturating_sub(start + prefix.len()) >= min_suffix_len {
                redacted.push_str(REDACTED_SECRET);
                index = end;
                continue;
            }
        }

        let ch = input[index..].chars().next().unwrap_or_default();
        redacted.push(ch);
        index += ch.len_utf8();
    }

    redacted
}

fn parse_event_uuid(
    raw: &str,
    code: &str,
    noun: &str,
    origin: &str,
) -> Result<Uuid, crate::core::error::HivemindError> {
    Uuid::parse_str(raw).map_err(|_| {
        crate::core::error::HivemindError::user(
            code,
            format!("'{raw}' is not a valid {noun} ID"),
            origin,
        )
    })
}

fn parse_event_time(
    raw: &str,
    flag: &str,
    origin: &str,
) -> Result<DateTime<Utc>, crate::core::error::HivemindError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            crate::core::error::HivemindError::user(
                "invalid_timestamp",
                format!("Invalid {flag} timestamp '{raw}'. Expected RFC3339 format."),
                origin,
            )
        })
}

fn parse_non_empty_filter(
    raw: &str,
    code: &str,
    flag: &str,
    origin: &str,
) -> Result<String, crate::core::error::HivemindError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(crate::core::error::HivemindError::user(
            code,
            format!("{flag} cannot be empty"),
            origin,
        ));
    }
    Ok(normalized.to_string())
}

#[allow(clippy::too_many_arguments)]
fn build_event_filter(
    registry: &Registry,
    origin: &str,
    project: Option<&str>,
    graph: Option<&str>,
    flow: Option<&str>,
    task: Option<&str>,
    attempt: Option<&str>,
    artifact_id: Option<&str>,
    template_id: Option<&str>,
    rule_id: Option<&str>,
    error_type: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
) -> Result<crate::storage::event_store::EventFilter, crate::core::error::HivemindError> {
    use crate::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.limit = Some(limit);

    if let Some(project) = project {
        filter.project_id = Some(registry.get_project(project)?.id);
    }
    if let Some(graph) = graph {
        filter.graph_id = Some(parse_event_uuid(
            graph,
            "invalid_graph_id",
            "graph",
            origin,
        )?);
    }
    if let Some(flow) = flow {
        filter.flow_id = Some(parse_event_uuid(flow, "invalid_flow_id", "flow", origin)?);
    }
    if let Some(task) = task {
        filter.task_id = Some(parse_event_uuid(task, "invalid_task_id", "task", origin)?);
    }
    if let Some(attempt) = attempt {
        filter.attempt_id = Some(parse_event_uuid(
            attempt,
            "invalid_attempt_id",
            "attempt",
            origin,
        )?);
    }
    if let Some(artifact_id) = artifact_id {
        filter.artifact_id = Some(parse_non_empty_filter(
            artifact_id,
            "invalid_artifact_id",
            "--artifact-id",
            origin,
        )?);
    }
    if let Some(template_id) = template_id {
        filter.template_id = Some(parse_non_empty_filter(
            template_id,
            "invalid_template_id",
            "--template-id",
            origin,
        )?);
    }
    if let Some(rule_id) = rule_id {
        filter.rule_id = Some(parse_non_empty_filter(
            rule_id,
            "invalid_rule_id",
            "--rule-id",
            origin,
        )?);
    }
    if let Some(error_type) = error_type {
        filter.error_type = Some(parse_non_empty_filter(
            error_type,
            "invalid_error_type",
            "--error-type",
            origin,
        )?);
    }
    if let Some(since) = since {
        filter.since = Some(parse_event_time(since, "--since", origin)?);
    }
    if let Some(until) = until {
        filter.until = Some(parse_event_time(until, "--until", origin)?);
    }

    if filter
        .since
        .zip(filter.until)
        .is_some_and(|(since, until)| since > until)
    {
        return Err(crate::core::error::HivemindError::user(
            "invalid_time_range",
            "`--since` must be earlier than or equal to `--until`",
            origin,
        ));
    }

    Ok(filter)
}

#[allow(clippy::too_many_lines)]
pub fn handle_events(cmd: EventCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        EventCommands::List(args) => {
            let filter = match build_event_filter(
                &registry,
                "cli:events:list",
                args.project.as_deref(),
                args.graph.as_deref(),
                args.flow.as_deref(),
                args.task.as_deref(),
                args.attempt.as_deref(),
                args.artifact_id.as_deref(),
                args.template_id.as_deref(),
                args.rule_id.as_deref(),
                args.error_type.as_deref(),
                args.since.as_deref(),
                args.until.as_deref(),
                args.limit,
            ) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            let events = match registry.read_events(&filter) {
                Ok(evs) => evs,
                Err(e) => return output_error(&e, format),
            };

            match format {
                OutputFormat::Json => {
                    let response = crate::cli::output::CliResponse::success(&events);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    let response = crate::cli::output::CliResponse::success(&events);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => print_events_table(events),
            }

            ExitCode::Success
        }
        EventCommands::Inspect(args) => {
            let event = match registry.get_event(&args.event_id) {
                Ok(ev) => ev,
                Err(e) => return output_error(&e, format),
            };

            match format {
                OutputFormat::Json => {
                    let response = crate::cli::output::CliResponse::success(&event);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&event) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => {
                    if let Ok(json) = serde_json::to_string_pretty(&event) {
                        println!("{json}");
                    }
                }
            }

            ExitCode::Success
        }
        EventCommands::Stream(args) => {
            let filter = match build_event_filter(
                &registry,
                "cli:events:stream",
                args.project.as_deref(),
                args.graph.as_deref(),
                args.flow.as_deref(),
                args.task.as_deref(),
                args.attempt.as_deref(),
                args.artifact_id.as_deref(),
                args.template_id.as_deref(),
                args.rule_id.as_deref(),
                args.error_type.as_deref(),
                args.since.as_deref(),
                args.until.as_deref(),
                args.limit,
            ) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            let rx = match registry.stream_events(&filter) {
                Ok(r) => r,
                Err(e) => return output_error(&e, format),
            };

            let mut events = Vec::new();
            let idle_timeout = std::time::Duration::from_millis(1000);
            while let Ok(ev) = rx.recv_timeout(idle_timeout) {
                events.push(ev);
                if events.len() >= args.limit {
                    break;
                }
            }
            if args.redact_secrets {
                events = redact_events_for_stream(events);
            }

            match format {
                OutputFormat::Json => {
                    let response = crate::cli::output::CliResponse::success(&events);
                    if let Ok(json) = serde_json::to_string_pretty(&response) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    let response = crate::cli::output::CliResponse::success(&events);
                    if let Ok(yaml) = serde_yaml::to_string(&response) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => print_events_table(events),
            }

            ExitCode::Success
        }
        EventCommands::Replay(args) => {
            let replayed = match registry.replay_flow(&args.flow_id) {
                Ok(f) => f,
                Err(e) => return output_error(&e, format),
            };

            if args.verify {
                let current = match registry.get_flow(&args.flow_id) {
                    Ok(f) => f,
                    Err(e) => return output_error(&e, format),
                };

                let match_ok = replayed.state == current.state
                    && replayed.task_executions.len() == current.task_executions.len()
                    && replayed.task_executions.iter().all(|(tid, exec)| {
                        current
                            .task_executions
                            .get(tid)
                            .is_some_and(|ce| ce.state == exec.state)
                    });

                if match_ok {
                    println!("Verification passed: replayed state matches current state.");
                } else {
                    eprintln!("Verification FAILED: replayed state differs from current state.");
                    return ExitCode::Error;
                }
            }

            match format {
                OutputFormat::Json => {
                    if let Ok(json) = serde_json::to_string_pretty(&replayed) {
                        println!("{json}");
                    }
                }
                OutputFormat::Yaml => {
                    if let Ok(yaml) = serde_yaml::to_string(&replayed) {
                        print!("{yaml}");
                    }
                }
                OutputFormat::Table => {
                    println!("ID:      {}", replayed.id);
                    println!("Graph:   {}", replayed.graph_id);
                    println!("State:   {:?}", replayed.state);
                    let mut counts = std::collections::HashMap::new();
                    for exec in replayed.task_executions.values() {
                        *counts.entry(exec.state).or_insert(0usize) += 1;
                    }
                    println!("Tasks:");
                    let mut keys: Vec<_> = counts.keys().copied().collect();
                    keys.sort_by_key(|k| format!("{k:?}"));
                    for k in keys {
                        println!("  - {:?}: {}", k, counts[&k]);
                    }
                }
            }

            ExitCode::Success
        }
        EventCommands::Verify(_args) => match registry.events_verify() {
            Ok(result) => {
                print_structured(&result, format, "events verify result");
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        EventCommands::Recover(args) => {
            if !args.from_mirror {
                return output_error(
                    &crate::core::error::HivemindError::user(
                        "events_recover_source_required",
                        "Specify an explicit source for recovery",
                        "cli:events:recover",
                    )
                    .with_hint("Use `hivemind events recover --from-mirror --confirm`"),
                    format,
                );
            }

            match registry.events_recover_from_mirror(args.confirm) {
                Ok(result) => {
                    print_structured(&result, format, "events recover result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}
