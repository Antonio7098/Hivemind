#![allow(
    clippy::implicit_clone,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

use super::*;
use chrono::DateTime;
use serde_json::{json, Map, Value};

enum RuntimeSessionSource {
    Fallback,
    Invocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalKind {
    Execution,
    Network,
}

#[derive(Debug, Clone)]
struct ApprovalProjectionState {
    view: RuntimeApprovalView,
}

impl Registry {
    pub fn attempt_runtime_projection(&self, attempt_id: &str) -> Result<AttemptRuntimeProjection> {
        let mut filter = EventFilter::all();
        filter.attempt_id = Some(parse_projection_uuid(
            attempt_id,
            "invalid_attempt_id",
            "server/runtime projection",
        )?);
        Ok(attempt_runtime_projection_from_events(
            &self.read_events(&filter)?,
        ))
    }

    pub fn runtime_stream_items(
        &self,
        flow_id: Option<&str>,
        attempt_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RuntimeStreamItemView>> {
        let mut filter = EventFilter::all();
        if let Some(flow_id) = flow_id {
            filter.flow_id = Some(parse_projection_uuid(
                flow_id,
                "invalid_flow_id",
                "registry:runtime_stream_items",
            )?);
        }
        if let Some(attempt_id) = attempt_id {
            filter.attempt_id = Some(parse_projection_uuid(
                attempt_id,
                "invalid_attempt_id",
                "registry:runtime_stream_items",
            )?);
        }

        Ok(runtime_stream_items_from_events(
            &self.read_events(&filter)?,
            limit,
        ))
    }

    pub fn runtime_stream_items_with_detail(
        &self,
        flow_id: Option<&str>,
        attempt_id: Option<&str>,
        limit: usize,
        detail: RuntimeStreamDetailLevel,
    ) -> Result<Vec<RuntimeStreamItemView>> {
        if matches!(detail, RuntimeStreamDetailLevel::Telemetry) {
            return self.runtime_stream_items(flow_id, attempt_id, limit);
        }

        let mut filter = EventFilter::all();
        if let Some(flow_id) = flow_id {
            filter.flow_id = Some(parse_projection_uuid(
                flow_id,
                "invalid_flow_id",
                "registry:runtime_stream_items",
            )?);
        }
        if let Some(attempt_id) = attempt_id {
            filter.attempt_id = Some(parse_projection_uuid(
                attempt_id,
                "invalid_attempt_id",
                "registry:runtime_stream_items",
            )?);
        }

        Ok(runtime_stream_items_from_events_with_detail(
            &self.read_events(&filter)?,
            limit,
            detail,
        ))
    }
}

pub(crate) fn attempt_runtime_projection_from_events(events: &[Event]) -> AttemptRuntimeProjection {
    let mut runtime_session = None;
    let mut runtime_session_source = None;
    let mut invocation_adapters = HashMap::new();
    let mut turn_refs = Vec::new();
    let mut next_turn_ordinal = 1u32;
    let mut approvals = approval_projection_from_events(events);

    for event in events {
        match &event.payload {
            EventPayload::RuntimeStarted {
                adapter_name,
                attempt_id,
                ..
            } => {
                if runtime_session.is_none() {
                    runtime_session = Some(AttemptRuntimeSessionView {
                        adapter_name: adapter_name.clone(),
                        session_id: attempt_id.to_string(),
                        discovered_at: event.metadata.timestamp,
                    });
                    runtime_session_source = Some(RuntimeSessionSource::Fallback);
                }
            }
            EventPayload::AgentInvocationStarted {
                invocation_id,
                adapter_name,
                ..
            } => {
                invocation_adapters.insert(invocation_id.clone(), adapter_name.clone());
                if !matches!(
                    runtime_session_source,
                    Some(RuntimeSessionSource::Invocation)
                ) {
                    runtime_session = Some(AttemptRuntimeSessionView {
                        adapter_name: adapter_name.clone(),
                        session_id: invocation_id.clone(),
                        discovered_at: event.metadata.timestamp,
                    });
                    runtime_session_source = Some(RuntimeSessionSource::Invocation);
                }
            }
            EventPayload::NativeTurnSummaryRecorded {
                invocation_id,
                turn_index,
                agent_mode,
                from_state,
                to_state,
                summary,
                ..
            } => {
                let adapter_name = invocation_adapters
                    .get(invocation_id)
                    .cloned()
                    .or_else(|| {
                        runtime_session
                            .as_ref()
                            .map(|session| session.adapter_name.clone())
                    })
                    .unwrap_or_else(|| "runtime".to_string());
                turn_refs.push(AttemptTurnRefView {
                    ordinal: next_turn_ordinal,
                    adapter_name,
                    stream: agent_mode.clone().unwrap_or_else(|| "native".to_string()),
                    provider_session_id: Some(invocation_id.clone()),
                    provider_turn_id: Some(turn_index.to_string()),
                    git_ref: None,
                    commit_sha: None,
                    summary: summary
                        .clone()
                        .or_else(|| Some(format!("{from_state} → {to_state}"))),
                });
                next_turn_ordinal += 1;
            }
            _ => {}
        }
    }

    approvals.sort_by_key(|approval| approval.requested_at);
    let pending_approvals = approvals
        .iter()
        .filter(|approval| approval.status == "pending")
        .cloned()
        .collect();

    AttemptRuntimeProjection {
        runtime_session,
        turn_refs,
        approvals,
        pending_approvals,
    }
}

pub(crate) fn runtime_stream_items_from_events(
    events: &[Event],
    limit: usize,
) -> Vec<RuntimeStreamItemView> {
    runtime_stream_items_from_events_with_detail(events, limit, RuntimeStreamDetailLevel::Telemetry)
}

pub(crate) fn runtime_stream_items_from_events_with_detail(
    events: &[Event],
    limit: usize,
    detail: RuntimeStreamDetailLevel,
) -> Vec<RuntimeStreamItemView> {
    let mut items = Vec::new();
    let mut pending_approvals: HashMap<String, Vec<RuntimeApprovalView>> = HashMap::new();
    for event in events {
        items.extend(runtime_stream_items_from_event(
            event,
            &mut pending_approvals,
        ));
    }
    items.retain(|item| detail.includes_kind(&item.kind));
    if limit > 0 && items.len() > limit {
        items.drain(0..items.len() - limit);
    }
    items
}

fn parse_projection_uuid(raw: &str, code: &'static str, origin: &'static str) -> Result<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| HivemindError::user(code, format!("'{raw}' is not a valid UUID"), origin))
}

fn runtime_stream_items_from_event(
    event: &Event,
    pending_approvals: &mut HashMap<String, Vec<RuntimeApprovalView>>,
) -> Vec<RuntimeStreamItemView> {
    let correlation = &event.metadata.correlation;
    let mk = |kind: &str,
              stream: Option<String>,
              title: Option<String>,
              text: Option<String>,
              data: Value| {
        RuntimeStreamItemView {
            event_id: event.metadata.id.to_string(),
            sequence: event.metadata.sequence.unwrap_or_default(),
            timestamp: event.metadata.timestamp,
            flow_id: correlation.flow_id.map(|id| id.to_string()),
            task_id: correlation.task_id.map(|id| id.to_string()),
            attempt_id: correlation.attempt_id.map(|id| id.to_string()),
            kind: kind.to_string(),
            stream,
            title,
            text,
            data: value_map(data),
        }
    };

    match &event.payload {
        EventPayload::RuntimeStarted {
            adapter_name,
            role,
            flags,
            ..
        } => vec![mk(
            "session",
            None,
            Some("Runtime started".to_string()),
            Some(format!(
                "{adapter_name} selected for {} execution",
                runtime_role_label(*role)
            )),
            json!({
                "adapter_name": adapter_name,
                "role": runtime_role_label(*role),
                "flags": flags,
            }),
        )],
        EventPayload::RuntimeEnvironmentPrepared {
            adapter_name,
            inherit_mode,
            inherited_keys,
            overlay_keys,
            ..
        } => vec![mk(
            "session",
            None,
            Some("Runtime environment prepared".to_string()),
            Some(format!(
                "{adapter_name} environment staged ({inherit_mode})"
            )),
            json!({
                "adapter_name": adapter_name,
                "inherit_mode": inherit_mode,
                "inherited_keys": inherited_keys,
                "overlay_keys": overlay_keys,
            }),
        )],
        EventPayload::AgentInvocationStarted {
            invocation_id,
            adapter_name,
            provider,
            model,
            runtime_version,
            agent_mode,
            ..
        } => vec![mk(
            "session",
            agent_mode.clone(),
            Some("Agent invocation started".to_string()),
            Some(format!("{provider}/{model} via {adapter_name}")),
            json!({
                "invocation_id": invocation_id,
                "adapter_name": adapter_name,
                "provider": provider,
                "model": model,
                "runtime_version": runtime_version,
                "agent_mode": agent_mode,
            }),
        )],
        EventPayload::NativeTurnSummaryRecorded {
            invocation_id,
            turn_index,
            agent_mode,
            from_state,
            to_state,
            summary,
            ..
        } => vec![mk(
            "turn",
            agent_mode.clone().or_else(|| Some("native".to_string())),
            Some(format!("Turn {}", turn_index + 1)),
            summary
                .clone()
                .or_else(|| Some(format!("{from_state} → {to_state}"))),
            json!({
                "invocation_id": invocation_id,
                "turn_index": turn_index,
                "from_state": from_state,
                "to_state": to_state,
                "agent_mode": agent_mode,
            }),
        )],
        EventPayload::AgentInvocationCompleted {
            invocation_id,
            total_turns,
            final_state,
            success,
            final_summary,
            error_code,
            error_message,
            ..
        } => vec![mk(
            "session",
            None,
            Some("Agent invocation completed".to_string()),
            final_summary.clone().or_else(|| Some(final_state.clone())),
            json!({
                "invocation_id": invocation_id,
                "total_turns": total_turns,
                "final_state": final_state,
                "success": success,
                "error_code": error_code,
                "error_message": error_message,
            }),
        )],
        EventPayload::RuntimeOutputChunk {
            stream, content, ..
        } => vec![mk(
            "output",
            Some(runtime_stream_label(*stream).to_string()),
            Some("Runtime output".to_string()),
            Some(content.clone()),
            json!({ "content": content }),
        )],
        EventPayload::RuntimeInputProvided { content, .. } => vec![mk(
            "input",
            None,
            Some("Runtime input provided".to_string()),
            Some(content.clone()),
            json!({ "content": content }),
        )],
        EventPayload::RuntimeInterrupted { .. } => vec![mk(
            "interrupt",
            None,
            Some("Runtime interrupted".to_string()),
            None,
            json!({}),
        )],
        EventPayload::RuntimeExited {
            exit_code,
            duration_ms,
            ..
        } => vec![mk(
            "runtime_exited",
            None,
            Some("Runtime exited".to_string()),
            Some(format!("exit {exit_code} after {duration_ms}ms")),
            json!({ "exit_code": exit_code, "duration_ms": duration_ms }),
        )],
        EventPayload::RuntimeTerminated { reason, .. } => vec![mk(
            "runtime_terminated",
            None,
            Some("Runtime terminated".to_string()),
            Some(reason.clone()),
            json!({ "reason": reason }),
        )],
        EventPayload::RuntimeErrorClassified {
            code,
            category,
            message,
            recoverable,
            retryable,
            rate_limited,
            ..
        } => vec![mk(
            "runtime_error",
            None,
            Some("Runtime error classified".to_string()),
            Some(message.clone()),
            json!({
                "code": code,
                "category": category,
                "recoverable": recoverable,
                "retryable": retryable,
                "rate_limited": rate_limited,
            }),
        )],
        EventPayload::RuntimeRecoveryScheduled {
            from_adapter,
            to_adapter,
            strategy,
            reason,
            backoff_ms,
            ..
        } => vec![mk(
            "recovery",
            None,
            Some("Runtime recovery scheduled".to_string()),
            Some(reason.clone()),
            json!({
                "from_adapter": from_adapter,
                "to_adapter": to_adapter,
                "strategy": strategy,
                "backoff_ms": backoff_ms,
            }),
        )],
        EventPayload::RuntimeFilesystemObserved {
            files_created,
            files_modified,
            files_deleted,
            ..
        } => vec![mk(
            "filesystem",
            None,
            Some("Filesystem observed".to_string()),
            Some(format!(
                "{} created · {} modified · {} deleted",
                files_created.len(),
                files_modified.len(),
                files_deleted.len()
            )),
            json!({
                "files_created": files_created,
                "files_modified": files_modified,
                "files_deleted": files_deleted,
            }),
        )],
        EventPayload::RuntimeCommandObserved {
            stream, command, ..
        } => vec![mk(
            "command",
            Some(runtime_stream_label(*stream).to_string()),
            Some("Command observed".to_string()),
            Some(command.clone()),
            json!({ "command": command }),
        )],
        EventPayload::RuntimeToolCallObserved {
            stream,
            tool_name,
            details,
            ..
        } => vec![mk(
            "tool_call",
            Some(runtime_stream_label(*stream).to_string()),
            Some(format!("Tool call: {tool_name}")),
            Some(details.clone()),
            json!({ "tool_name": tool_name, "details": details }),
        )],
        EventPayload::ToolCallRequested {
            invocation_id,
            turn_index,
            call_id,
            tool_name,
            policy_tags,
            ..
        } => {
            let mut items = Vec::new();
            for approval in requested_approvals_from_policy_tags(
                event.metadata.timestamp,
                invocation_id,
                *turn_index,
                call_id,
                tool_name,
                policy_tags,
            ) {
                if approval.status == "pending" {
                    pending_approvals
                        .entry(call_id.to_string())
                        .or_default()
                        .push(approval.clone());
                }
                items.push(mk(
                    "approval",
                    None,
                    Some(approval_stream_title(&approval)),
                    approval.summary.clone(),
                    json!({
                        "approval_id": approval.approval_id,
                        "call_id": approval.call_id,
                        "invocation_id": approval.invocation_id,
                        "turn_index": approval.turn_index,
                        "tool_name": approval.tool_name,
                        "approval_kind": approval.approval_kind,
                        "status": approval.status,
                        "resource": approval.resource,
                        "decision": approval.decision,
                        "policy_tags": approval.policy_tags,
                    }),
                ));
            }
            if items.is_empty() {
                items.push(mk(
                    "tool_call_requested",
                    None,
                    Some(format!("Tool requested: {tool_name}")),
                    Some(format!("turn {} · {call_id}", turn_index + 1)),
                    json!({
                        "call_id": call_id,
                        "invocation_id": invocation_id,
                        "turn_index": turn_index,
                        "tool_name": tool_name,
                        "policy_tags": policy_tags,
                    }),
                ));
            }
            items
        }
        EventPayload::ToolCallStarted {
            invocation_id,
            turn_index,
            call_id,
            tool_name,
            policy_tags,
            ..
        } => vec![mk(
            "tool_call_started",
            None,
            Some(format!("Tool started: {tool_name}")),
            Some(format!("turn {} · {call_id}", turn_index + 1)),
            json!({
                "call_id": call_id,
                "invocation_id": invocation_id,
                "turn_index": turn_index,
                "tool_name": tool_name,
                "policy_tags": policy_tags,
            }),
        )],
        EventPayload::ToolCallCompleted {
            invocation_id,
            turn_index,
            call_id,
            tool_name,
            duration_ms,
            policy_tags,
            ..
        } => completed_approval_items(
            event,
            invocation_id,
            *turn_index,
            call_id,
            tool_name,
            take_pending_approvals(pending_approvals, call_id),
            *duration_ms,
            policy_tags,
        ),
        EventPayload::ToolCallFailed {
            invocation_id,
            turn_index,
            call_id,
            tool_name,
            duration_ms,
            message,
            denial_reason,
            policy_tags,
            ..
        } => failed_approval_items(
            event,
            invocation_id,
            *turn_index,
            call_id,
            tool_name,
            take_pending_approvals(pending_approvals, call_id),
            *duration_ms,
            message,
            denial_reason.as_deref(),
            policy_tags,
        ),
        EventPayload::RuntimeTodoSnapshotUpdated { stream, items, .. } => vec![mk(
            "todo",
            Some(runtime_stream_label(*stream).to_string()),
            Some("Todo snapshot updated".to_string()),
            Some(format!("{} items tracked", items.len())),
            json!({ "items": items }),
        )],
        EventPayload::RuntimeNarrativeOutputObserved {
            stream, content, ..
        } => vec![mk(
            "narrative",
            Some(runtime_stream_label(*stream).to_string()),
            Some("Narrative output".to_string()),
            Some(content.clone()),
            json!({ "content": content }),
        )],
        EventPayload::CheckpointDeclared {
            checkpoint_id,
            order,
            total,
            ..
        } => vec![mk(
            "checkpoint_declared",
            None,
            Some(format!("Checkpoint declared: {checkpoint_id}")),
            Some(format!("checkpoint {order} of {total}")),
            json!({ "checkpoint_id": checkpoint_id, "order": order, "total": total }),
        )],
        EventPayload::CheckpointActivated {
            checkpoint_id,
            order,
            ..
        } => vec![mk(
            "checkpoint_activated",
            None,
            Some(format!("Checkpoint activated: {checkpoint_id}")),
            Some(format!("checkpoint {order} active")),
            json!({ "checkpoint_id": checkpoint_id, "order": order }),
        )],
        EventPayload::CheckpointCompleted {
            checkpoint_id,
            order,
            commit_hash,
            summary,
            ..
        } => vec![mk(
            "checkpoint_completed",
            None,
            Some(format!("Checkpoint completed: {checkpoint_id}")),
            summary
                .clone()
                .or_else(|| Some(format!("checkpoint {order} completed"))),
            json!({
                "checkpoint_id": checkpoint_id,
                "order": order,
                "commit_hash": commit_hash,
                "summary": summary,
            }),
        )],
        EventPayload::AllCheckpointsCompleted { .. } => vec![mk(
            "checkpoint_all_completed",
            None,
            Some("All checkpoints completed".to_string()),
            None,
            json!({}),
        )],
        EventPayload::CheckpointCommitCreated { commit_sha, .. } => vec![mk(
            "checkpoint_commit",
            None,
            Some("Checkpoint commit created".to_string()),
            Some(commit_sha.clone()),
            json!({ "commit_sha": commit_sha }),
        )],
        _ => vec![],
    }
}

fn approval_projection_from_events(events: &[Event]) -> Vec<RuntimeApprovalView> {
    let mut approvals: HashMap<String, ApprovalProjectionState> = HashMap::new();

    for event in events {
        match &event.payload {
            EventPayload::ToolCallRequested {
                invocation_id,
                turn_index,
                call_id,
                tool_name,
                policy_tags,
                ..
            } => {
                for approval in requested_approvals_from_policy_tags(
                    event.metadata.timestamp,
                    invocation_id,
                    *turn_index,
                    call_id,
                    tool_name,
                    policy_tags,
                ) {
                    approvals.insert(
                        approval.approval_id.clone(),
                        ApprovalProjectionState { view: approval },
                    );
                }
            }
            EventPayload::ToolCallCompleted {
                call_id,
                duration_ms,
                policy_tags,
                ..
            } => {
                resolve_pending_approvals(
                    &mut approvals,
                    call_id,
                    policy_tags,
                    event.metadata.timestamp,
                    None,
                    *duration_ms,
                    false,
                );
            }
            EventPayload::ToolCallFailed {
                call_id,
                message,
                denial_reason,
                duration_ms,
                policy_tags,
                ..
            } => {
                resolve_pending_approvals(
                    &mut approvals,
                    call_id,
                    policy_tags,
                    event.metadata.timestamp,
                    denial_reason.as_deref().or(Some(message.as_str())),
                    *duration_ms,
                    true,
                );
            }
            _ => {}
        }
    }

    let mut views: Vec<_> = approvals.into_values().map(|state| state.view).collect();
    views.sort_by_key(|approval| approval.requested_at);
    views
}

fn requested_approvals_from_policy_tags(
    requested_at: DateTime<Utc>,
    invocation_id: &str,
    turn_index: u32,
    call_id: &str,
    tool_name: &str,
    policy_tags: &[String],
) -> Vec<RuntimeApprovalView> {
    let mut approvals = Vec::new();

    if tag_is_true(policy_tags, "approval_required") {
        approvals.push(RuntimeApprovalView {
            approval_id: approval_id(call_id, ApprovalKind::Execution),
            call_id: call_id.to_string(),
            invocation_id: invocation_id.to_string(),
            turn_index,
            tool_name: tool_name.to_string(),
            approval_kind: approval_kind_label(ApprovalKind::Execution).to_string(),
            status: execution_approval_status(policy_tags).to_string(),
            resource: first_policy_tag_value(policy_tags, "exec_danger_reason")
                .map(str::to_string)
                .or_else(|| Some(tool_name.to_string())),
            decision: first_policy_tag_value(policy_tags, "approval_outcome").map(str::to_string),
            summary: approval_summary(tool_name, ApprovalKind::Execution, policy_tags, None),
            requested_at,
            resolved_at: immediate_approval_resolution(policy_tags, ApprovalKind::Execution)
                .then_some(requested_at),
            policy_tags: policy_tags.to_vec(),
        });
    }

    if tag_is_true(policy_tags, "network_approval_required") {
        approvals.push(RuntimeApprovalView {
            approval_id: approval_id(call_id, ApprovalKind::Network),
            call_id: call_id.to_string(),
            invocation_id: invocation_id.to_string(),
            turn_index,
            tool_name: tool_name.to_string(),
            approval_kind: approval_kind_label(ApprovalKind::Network).to_string(),
            status: network_approval_status(policy_tags).to_string(),
            resource: first_policy_tag_value(policy_tags, "network_target")
                .map(str::to_string)
                .or_else(|| Some(tool_name.to_string())),
            decision: first_policy_tag_value(policy_tags, "network_approval_outcome")
                .map(str::to_string),
            summary: approval_summary(tool_name, ApprovalKind::Network, policy_tags, None),
            requested_at,
            resolved_at: immediate_approval_resolution(policy_tags, ApprovalKind::Network)
                .then_some(requested_at),
            policy_tags: policy_tags.to_vec(),
        });
    }

    approvals
}

fn resolve_pending_approvals(
    approvals: &mut HashMap<String, ApprovalProjectionState>,
    call_id: &str,
    policy_tags: &[String],
    resolved_at: DateTime<Utc>,
    failure_reason: Option<&str>,
    duration_ms: Option<u64>,
    failed: bool,
) {
    for kind in [ApprovalKind::Execution, ApprovalKind::Network] {
        let approval_id = approval_id(call_id, kind);
        let Some(state) = approvals.get_mut(&approval_id) else {
            continue;
        };
        if state.view.status != "pending" {
            continue;
        }

        let resolved_status = if kind == ApprovalKind::Network {
            if failed {
                network_resolution_status(policy_tags, failure_reason)
            } else {
                "approved"
            }
        } else if failed {
            execution_resolution_status(policy_tags, failure_reason)
        } else {
            "approved"
        };

        state.view.status = resolved_status.to_string();
        state.view.resolved_at = Some(resolved_at);
        if state.view.decision.is_none() {
            state.view.decision =
                approval_resolution_decision(kind, policy_tags, failed).map(str::to_string);
        }
        if let Some(summary) =
            approval_summary(&state.view.tool_name, kind, policy_tags, failure_reason)
        {
            state.view.summary = Some(with_duration(summary, duration_ms));
        }
    }
}

fn completed_approval_items(
    event: &Event,
    invocation_id: &str,
    turn_index: u32,
    call_id: &str,
    tool_name: &str,
    pending_approvals: Vec<RuntimeApprovalView>,
    duration_ms: Option<u64>,
    policy_tags: &[String],
) -> Vec<RuntimeStreamItemView> {
    let correlation = &event.metadata.correlation;
    let mut items = Vec::new();
    for approval in pending_approvals {
        let kind = approval_kind_enum(&approval.approval_kind);
        let status = match kind {
            ApprovalKind::Execution => execution_resolution_status(policy_tags, None),
            ApprovalKind::Network => network_resolution_status(policy_tags, None),
        };
        let mut resolved_approval = approval.clone();
        resolved_approval.status = status.to_string();
        resolved_approval.decision =
            approval_resolution_decision(kind, policy_tags, false).map(str::to_string);
        items.push(RuntimeStreamItemView {
            event_id: event.metadata.id.to_string(),
            sequence: event.metadata.sequence.unwrap_or_default(),
            timestamp: event.metadata.timestamp,
            flow_id: correlation.flow_id.map(|id| id.to_string()),
            task_id: correlation.task_id.map(|id| id.to_string()),
            attempt_id: correlation.attempt_id.map(|id| id.to_string()),
            kind: "approval".to_string(),
            stream: None,
            title: Some(approval_stream_title(&resolved_approval)),
            text: approval_summary(tool_name, kind, policy_tags, None)
                .map(|summary| with_duration(summary, duration_ms)),
            data: value_map(json!({
                "approval_id": resolved_approval.approval_id,
                "call_id": resolved_approval.call_id,
                "invocation_id": resolved_approval.invocation_id,
                "turn_index": resolved_approval.turn_index,
                "tool_name": resolved_approval.tool_name,
                "approval_kind": resolved_approval.approval_kind,
                "status": resolved_approval.status,
                "resource": resolved_approval.resource,
                "decision": resolved_approval.decision,
                "policy_tags": policy_tags,
            })),
        });
    }
    items.push(RuntimeStreamItemView {
        event_id: event.metadata.id.to_string(),
        sequence: event.metadata.sequence.unwrap_or_default(),
        timestamp: event.metadata.timestamp,
        flow_id: correlation.flow_id.map(|id| id.to_string()),
        task_id: correlation.task_id.map(|id| id.to_string()),
        attempt_id: correlation.attempt_id.map(|id| id.to_string()),
        kind: "tool_call_completed".to_string(),
        stream: None,
        title: Some(format!("Tool completed: {tool_name}")),
        text: Some(with_duration(
            format!("turn {} · {call_id}", turn_index + 1),
            duration_ms,
        )),
        data: value_map(json!({
            "call_id": call_id,
            "invocation_id": invocation_id,
            "turn_index": turn_index,
            "tool_name": tool_name,
            "duration_ms": duration_ms,
            "policy_tags": policy_tags,
        })),
    });
    items
}

fn failed_approval_items(
    event: &Event,
    invocation_id: &str,
    turn_index: u32,
    call_id: &str,
    tool_name: &str,
    pending_approvals: Vec<RuntimeApprovalView>,
    duration_ms: Option<u64>,
    message: &str,
    denial_reason: Option<&str>,
    policy_tags: &[String],
) -> Vec<RuntimeStreamItemView> {
    let correlation = &event.metadata.correlation;
    let mut items = Vec::new();
    for approval in pending_approvals {
        let kind = approval_kind_enum(&approval.approval_kind);
        let status = if kind == ApprovalKind::Network {
            network_resolution_status(policy_tags, denial_reason.or(Some(message)))
        } else {
            execution_resolution_status(policy_tags, denial_reason.or(Some(message)))
        };
        let mut resolved_approval = approval.clone();
        resolved_approval.status = status.to_string();
        resolved_approval.decision =
            approval_resolution_decision(kind, policy_tags, true).map(str::to_string);
        items.push(RuntimeStreamItemView {
            event_id: event.metadata.id.to_string(),
            sequence: event.metadata.sequence.unwrap_or_default(),
            timestamp: event.metadata.timestamp,
            flow_id: correlation.flow_id.map(|id| id.to_string()),
            task_id: correlation.task_id.map(|id| id.to_string()),
            attempt_id: correlation.attempt_id.map(|id| id.to_string()),
            kind: "approval".to_string(),
            stream: None,
            title: Some(approval_stream_title(&resolved_approval)),
            text: approval_summary(
                tool_name,
                kind,
                policy_tags,
                denial_reason.or(Some(message)),
            ),
            data: value_map(json!({
                "approval_id": resolved_approval.approval_id,
                "call_id": resolved_approval.call_id,
                "invocation_id": resolved_approval.invocation_id,
                "turn_index": resolved_approval.turn_index,
                "tool_name": resolved_approval.tool_name,
                "approval_kind": resolved_approval.approval_kind,
                "status": resolved_approval.status,
                "resource": resolved_approval.resource,
                "decision": resolved_approval.decision,
                "policy_tags": policy_tags,
            })),
        });
    }
    items.push(RuntimeStreamItemView {
        event_id: event.metadata.id.to_string(),
        sequence: event.metadata.sequence.unwrap_or_default(),
        timestamp: event.metadata.timestamp,
        flow_id: correlation.flow_id.map(|id| id.to_string()),
        task_id: correlation.task_id.map(|id| id.to_string()),
        attempt_id: correlation.attempt_id.map(|id| id.to_string()),
        kind: "tool_call_failed".to_string(),
        stream: None,
        title: Some(format!("Tool failed: {tool_name}")),
        text: Some(with_duration(
            denial_reason.unwrap_or(message).to_string(),
            duration_ms,
        )),
        data: value_map(json!({
            "call_id": call_id,
            "invocation_id": invocation_id,
            "turn_index": turn_index,
            "tool_name": tool_name,
            "duration_ms": duration_ms,
            "message": message,
            "denial_reason": denial_reason,
            "policy_tags": policy_tags,
        })),
    });
    items
}

fn take_pending_approvals(
    pending_approvals: &mut HashMap<String, Vec<RuntimeApprovalView>>,
    call_id: &str,
) -> Vec<RuntimeApprovalView> {
    pending_approvals.remove(call_id).unwrap_or_default()
}

fn approval_id(call_id: &str, kind: ApprovalKind) -> String {
    format!("{call_id}:{}", approval_kind_label(kind))
}

fn approval_kind_label(kind: ApprovalKind) -> &'static str {
    match kind {
        ApprovalKind::Execution => "execution",
        ApprovalKind::Network => "network",
    }
}

fn approval_kind_enum(label: &str) -> ApprovalKind {
    match label {
        "network" => ApprovalKind::Network,
        _ => ApprovalKind::Execution,
    }
}

fn approval_stream_title(approval: &RuntimeApprovalView) -> String {
    format!(
        "{} approval {}",
        capitalize_label(&approval.approval_kind),
        approval.status
    )
}

fn capitalize_label(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => value.to_string(),
    }
}

fn first_policy_tag_value<'a>(policy_tags: &'a [String], key: &str) -> Option<&'a str> {
    policy_tags.iter().find_map(|tag| {
        let (tag_key, tag_value) = tag.split_once(':')?;
        (tag_key == key).then_some(tag_value)
    })
}

fn tag_is_true(policy_tags: &[String], key: &str) -> bool {
    matches!(first_policy_tag_value(policy_tags, key), Some("true"))
}

fn execution_approval_status(policy_tags: &[String]) -> &'static str {
    match first_policy_tag_value(policy_tags, "approval_outcome") {
        Some("denied" | "denied_broad_prefix") => "denied",
        Some("approved_for_session") => "approved",
        _ => "pending",
    }
}

fn network_approval_status(policy_tags: &[String]) -> &'static str {
    match first_policy_tag_value(policy_tags, "network_approval_outcome") {
        Some("approved_for_session" | "approved_cached") => "approved",
        Some("denied" | "denied_no_watcher" | "deferred_denied") => "denied",
        _ => "pending",
    }
}

fn immediate_approval_resolution(policy_tags: &[String], kind: ApprovalKind) -> bool {
    match kind {
        ApprovalKind::Execution => execution_approval_status(policy_tags) != "pending",
        ApprovalKind::Network => network_approval_status(policy_tags) != "pending",
    }
}

fn approval_summary(
    tool_name: &str,
    kind: ApprovalKind,
    policy_tags: &[String],
    failure_reason: Option<&str>,
) -> Option<String> {
    match kind {
        ApprovalKind::Execution => {
            let resource = first_policy_tag_value(policy_tags, "exec_danger_reason")
                .unwrap_or(tool_name)
                .to_string();
            let status = execution_approval_status(policy_tags);
            Some(match status {
                "approved" => format!("Execution approval granted for {resource}"),
                "denied" => format!(
                    "Execution approval denied for {resource}: {}",
                    failure_reason
                        .or_else(|| first_policy_tag_value(policy_tags, "approval_review_decision"))
                        .unwrap_or("policy denied")
                ),
                _ => format!("Execution approval pending for {resource}"),
            })
        }
        ApprovalKind::Network => {
            let target = first_policy_tag_value(policy_tags, "network_target")
                .unwrap_or(tool_name)
                .to_string();
            let status = network_approval_status(policy_tags);
            Some(match status {
                "approved" => format!("Network access approved for {target}"),
                "denied" => format!(
                    "Network access denied for {target}: {}",
                    failure_reason.unwrap_or("policy denied")
                ),
                _ => format!("Network approval pending for {target}"),
            })
        }
    }
}

fn approval_resolution_decision(
    kind: ApprovalKind,
    policy_tags: &[String],
    failed: bool,
) -> Option<&str> {
    match kind {
        ApprovalKind::Execution => {
            first_policy_tag_value(policy_tags, "approval_outcome").or(if failed {
                Some("denied")
            } else {
                Some("approved")
            })
        }
        ApprovalKind::Network => first_policy_tag_value(policy_tags, "network_approval_outcome")
            .or(if failed {
                Some("denied")
            } else {
                Some("approved")
            }),
    }
}

fn network_resolution_status(policy_tags: &[String], failure_reason: Option<&str>) -> &'static str {
    match first_policy_tag_value(policy_tags, "network_approval_outcome") {
        Some("deferred_denied" | "denied" | "denied_no_watcher") => "denied",
        Some("approved_for_session" | "approved_cached") => "approved",
        _ => {
            if failure_reason.is_some() {
                "approved"
            } else {
                "pending"
            }
        }
    }
}

fn execution_resolution_status(
    policy_tags: &[String],
    failure_reason: Option<&str>,
) -> &'static str {
    match first_policy_tag_value(policy_tags, "approval_outcome") {
        Some("denied" | "denied_broad_prefix") => "denied",
        Some("approved_for_session") => "approved",
        _ => {
            if failure_reason.is_some() {
                "denied"
            } else {
                "approved"
            }
        }
    }
}

fn with_duration(summary: String, duration_ms: Option<u64>) -> String {
    match duration_ms {
        Some(duration_ms) => format!("{summary} · {duration_ms}ms"),
        None => summary,
    }
}

fn value_map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn runtime_role_label(role: RuntimeRole) -> &'static str {
    match role {
        RuntimeRole::Worker => "worker",
        RuntimeRole::Validator => "validator",
    }
}

fn runtime_stream_label(stream: RuntimeOutputStream) -> &'static str {
    match stream {
        RuntimeOutputStream::Stdout => "stdout",
        RuntimeOutputStream::Stderr => "stderr",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence: u64, correlation: CorrelationIds, payload: EventPayload) -> Event {
        let mut event = Event::new(payload, correlation);
        event.metadata.sequence = Some(sequence);
        event
    }

    fn attempt_correlation(
        project_id: Uuid,
        graph_id: Uuid,
        flow_id: Uuid,
        task_id: Uuid,
        attempt_id: Uuid,
    ) -> CorrelationIds {
        CorrelationIds {
            project_id: Some(project_id),
            graph_id: Some(graph_id),
            flow_id: Some(flow_id),
            workflow_id: None,
            workflow_run_id: None,
            root_workflow_run_id: None,
            parent_workflow_run_id: None,
            task_id: Some(task_id),
            step_id: None,
            step_run_id: None,
            attempt_id: Some(attempt_id),
        }
    }

    fn native_blob_ref(label: &str) -> NativeBlobRef {
        NativeBlobRef {
            digest: format!("digest-{label}"),
            byte_size: label.len() as u64,
            media_type: "application/json".to_string(),
            blob_path: format!("blobs/{label}.json"),
            payload: Some(format!(r#"{{"label":"{label}"}}"#)),
        }
    }

    #[test]
    fn derives_attempt_runtime_session_and_turn_refs() {
        let project_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let correlation = attempt_correlation(project_id, graph_id, flow_id, task_id, attempt_id);
        let events = vec![
            event(
                1,
                correlation.clone(),
                EventPayload::RuntimeStarted {
                    adapter_name: "native".to_string(),
                    role: RuntimeRole::Worker,
                    task_id,
                    attempt_id,
                    prompt: String::new(),
                    flags: vec![],
                },
            ),
            event(
                2,
                correlation.clone(),
                EventPayload::AgentInvocationStarted {
                    native_correlation: NativeEventCorrelation {
                        project_id,
                        graph_id,
                        flow_id,
                        task_id,
                        attempt_id,
                    },
                    invocation_id: "inv-1".to_string(),
                    adapter_name: "native".to_string(),
                    provider: "mock".to_string(),
                    model: "mock-model".to_string(),
                    runtime_version: "1.0".to_string(),
                    capture_mode: NativeEventPayloadCaptureMode::MetadataOnly,
                    agent_mode: Some("plan".to_string()),
                    allowed_tools: vec![],
                    allowed_capabilities: vec![],
                    configured_max_turns: Some(4),
                    configured_timeout_budget_ms: None,
                    configured_token_budget: None,
                    configured_prompt_headroom: None,
                },
            ),
            event(
                3,
                correlation,
                EventPayload::NativeTurnSummaryRecorded {
                    native_correlation: NativeEventCorrelation {
                        project_id,
                        graph_id,
                        flow_id,
                        task_id,
                        attempt_id,
                    },
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    agent_mode: Some("plan".to_string()),
                    from_state: "thinking".to_string(),
                    to_state: "completed".to_string(),
                    prompt_hash: None,
                    context_manifest_hash: None,
                    delivered_context_hash: None,
                    mode_contract_hash: None,
                    inputs_hash: None,
                    prompt_headroom: None,
                    available_budget: 0,
                    rendered_prompt_bytes: 0,
                    runtime_context_bytes: 0,
                    static_prompt_chars: 0,
                    selected_history_chars: 0,
                    compacted_summary_chars: 0,
                    code_navigation_chars: 0,
                    tool_contract_chars: 0,
                    assembly_duration_ms: 0,
                    visible_item_count: 0,
                    selected_history_count: 0,
                    code_navigation_count: 0,
                    compacted_summary_count: 0,
                    tool_contract_count: 0,
                    skipped_item_count: 0,
                    truncated_item_count: 0,
                    tool_result_items_visible: 0,
                    latest_tool_result_turn_index: None,
                    latest_tool_names_visible: vec![],
                    active_code_window_trace: Vec::new(),
                    tool_call_count: 0,
                    tool_failure_count: 0,
                    model_latency_ms: 0,
                    tool_latency_ms: 0,
                    turn_duration_ms: 0,
                    elapsed_since_invocation_ms: 0,
                    request_tokens: 0,
                    response_tokens: 0,
                    budget_used_before: 0,
                    budget_used_after: 0,
                    budget_remaining: 0,
                    budget_thresholds_crossed: vec![],
                    summary: Some("Planned the change".to_string()),
                },
            ),
        ];

        let projection = attempt_runtime_projection_from_events(&events);
        let session = projection.runtime_session.expect("runtime session");
        assert_eq!(session.adapter_name, "native");
        assert_eq!(session.session_id, "inv-1");
        assert_eq!(projection.turn_refs.len(), 1);
        assert_eq!(
            projection.turn_refs[0].provider_turn_id.as_deref(),
            Some("0")
        );
        assert_eq!(
            projection.turn_refs[0].summary.as_deref(),
            Some("Planned the change")
        );
    }

    #[test]
    fn projects_runtime_stream_items_and_applies_limit() {
        let project_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let correlation = attempt_correlation(project_id, graph_id, flow_id, task_id, attempt_id);
        let events = vec![
            event(
                1,
                correlation.clone(),
                EventPayload::RuntimeStarted {
                    adapter_name: "native".to_string(),
                    role: RuntimeRole::Worker,
                    task_id,
                    attempt_id,
                    prompt: String::new(),
                    flags: vec![],
                },
            ),
            event(
                2,
                correlation.clone(),
                EventPayload::RuntimeCommandObserved {
                    attempt_id,
                    stream: RuntimeOutputStream::Stdout,
                    command: "cargo test".to_string(),
                },
            ),
            event(
                3,
                correlation,
                EventPayload::CheckpointCompleted {
                    flow_id,
                    task_id,
                    attempt_id,
                    checkpoint_id: "verify".to_string(),
                    order: 1,
                    commit_hash: "abc123".to_string(),
                    timestamp: Utc::now(),
                    summary: Some("verification complete".to_string()),
                },
            ),
        ];

        let items = runtime_stream_items_from_events(&events, 2);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "command");
        assert_eq!(items[1].kind, "checkpoint_completed");
        assert_eq!(items[1].sequence, 3);
    }

    #[test]
    fn runtime_stream_detail_levels_filter_items() {
        let project_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let correlation = attempt_correlation(project_id, graph_id, flow_id, task_id, attempt_id);
        let native_correlation = NativeEventCorrelation {
            project_id,
            graph_id,
            flow_id,
            task_id,
            attempt_id,
        };
        let events = vec![
            event(
                1,
                correlation.clone(),
                EventPayload::RuntimeStarted {
                    adapter_name: "native".to_string(),
                    role: RuntimeRole::Worker,
                    task_id,
                    attempt_id,
                    prompt: String::new(),
                    flags: vec![],
                },
            ),
            event(
                2,
                correlation.clone(),
                EventPayload::RuntimeCommandObserved {
                    attempt_id,
                    stream: RuntimeOutputStream::Stdout,
                    command: "cargo test".to_string(),
                },
            ),
            event(
                3,
                correlation.clone(),
                EventPayload::RuntimeOutputChunk {
                    attempt_id,
                    stream: RuntimeOutputStream::Stdout,
                    content: "running tests".to_string(),
                },
            ),
            event(
                4,
                correlation.clone(),
                EventPayload::ToolCallStarted {
                    native_correlation,
                    task_id: Some(task_id),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    call_id: "call-1".to_string(),
                    tool_name: "read_file".to_string(),
                    policy_tags: vec![],
                },
            ),
            event(
                5,
                correlation,
                EventPayload::RuntimeExited {
                    attempt_id,
                    exit_code: 0,
                    duration_ms: 42,
                },
            ),
        ];

        let summary = runtime_stream_items_from_events_with_detail(
            &events,
            20,
            RuntimeStreamDetailLevel::Summary,
        );
        assert_eq!(
            summary
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "runtime_exited"]
        );

        let observability = runtime_stream_items_from_events_with_detail(
            &events,
            20,
            RuntimeStreamDetailLevel::Observability,
        );
        assert_eq!(
            observability
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["session", "command", "tool_call_started", "runtime_exited"]
        );

        let telemetry = runtime_stream_items_from_events_with_detail(
            &events,
            20,
            RuntimeStreamDetailLevel::Telemetry,
        );
        assert_eq!(
            telemetry
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "session",
                "command",
                "output",
                "tool_call_started",
                "runtime_exited",
            ]
        );
    }

    #[test]
    fn derives_runtime_approvals_and_pending_approvals() {
        let project_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let correlation = attempt_correlation(project_id, graph_id, flow_id, task_id, attempt_id);
        let native_correlation = NativeEventCorrelation {
            project_id,
            graph_id,
            flow_id,
            task_id,
            attempt_id,
        };
        let events = vec![
            event(
                1,
                correlation.clone(),
                EventPayload::ToolCallRequested {
                    native_correlation: native_correlation.clone(),
                    task_id: Some(task_id),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    call_id: "call-exec".to_string(),
                    tool_name: "run_command".to_string(),
                    request: native_blob_ref("call-exec-request"),
                    policy_tags: vec![
                        "approval_required:true".to_string(),
                        "approval_outcome:approved_for_session".to_string(),
                        "exec_danger_reason:write_outside_workspace".to_string(),
                    ],
                },
            ),
            event(
                2,
                correlation,
                EventPayload::ToolCallRequested {
                    native_correlation,
                    task_id: Some(task_id),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 1,
                    call_id: "call-net".to_string(),
                    tool_name: "web_fetch".to_string(),
                    request: native_blob_ref("call-net-request"),
                    policy_tags: vec![
                        "network_approval_required:true".to_string(),
                        "network_target:api.github.com:443".to_string(),
                        "network_approval_outcome:deferred_pending".to_string(),
                    ],
                },
            ),
        ];

        let projection = attempt_runtime_projection_from_events(&events);
        assert_eq!(projection.approvals.len(), 2);
        assert_eq!(projection.pending_approvals.len(), 1);
        assert_eq!(projection.pending_approvals[0].approval_kind, "network");
        assert_eq!(projection.pending_approvals[0].status, "pending");
    }

    #[test]
    fn projects_runtime_stream_approval_items() {
        let project_id = Uuid::new_v4();
        let graph_id = Uuid::new_v4();
        let flow_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let attempt_id = Uuid::new_v4();
        let correlation = attempt_correlation(project_id, graph_id, flow_id, task_id, attempt_id);
        let native_correlation = NativeEventCorrelation {
            project_id,
            graph_id,
            flow_id,
            task_id,
            attempt_id,
        };
        let events = vec![
            event(
                1,
                correlation.clone(),
                EventPayload::ToolCallRequested {
                    native_correlation: native_correlation.clone(),
                    task_id: Some(task_id),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    call_id: "call-net".to_string(),
                    tool_name: "web_fetch".to_string(),
                    request: native_blob_ref("call-net-request"),
                    policy_tags: vec![
                        "network_approval_required:true".to_string(),
                        "network_target:api.github.com:443".to_string(),
                        "network_approval_outcome:deferred_pending".to_string(),
                    ],
                },
            ),
            event(
                2,
                correlation,
                EventPayload::ToolCallCompleted {
                    native_correlation,
                    task_id: Some(task_id),
                    invocation_id: "inv-1".to_string(),
                    turn_index: 0,
                    call_id: "call-net".to_string(),
                    tool_name: "web_fetch".to_string(),
                    response: native_blob_ref("call-net-response"),
                    duration_ms: Some(17),
                    policy_tags: vec![
                        "network_approval_required:true".to_string(),
                        "network_target:api.github.com:443".to_string(),
                        "network_approval_outcome:approved_for_session".to_string(),
                    ],
                },
            ),
        ];

        let items = runtime_stream_items_from_events(&events, 10);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, "approval");
        assert_eq!(items[0].data["status"], "pending");
        assert_eq!(items[1].kind, "approval");
        assert_eq!(items[1].data["status"], "approved");
        assert_eq!(items[2].kind, "tool_call_completed");
    }
}
