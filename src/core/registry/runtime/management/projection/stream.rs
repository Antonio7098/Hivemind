use super::*;
use crate::core::events::{Event, EventPayload, RuntimeOutputStream, RuntimeRole};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

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
            for approval in super::approval::requested_approvals_from_policy_tags(
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
                    Some(super::approval::approval_stream_title(&approval)),
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
        } => super::approval::completed_approval_items(
            event,
            invocation_id,
            *turn_index,
            call_id,
            tool_name,
            super::approval::take_pending_approvals(pending_approvals, call_id),
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
        } => super::approval::failed_approval_items(
            event,
            invocation_id,
            *turn_index,
            call_id,
            tool_name,
            super::approval::take_pending_approvals(pending_approvals, call_id),
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

pub(super) fn with_duration(summary: String, duration_ms: Option<u64>) -> String {
    match duration_ms {
        Some(duration_ms) => format!("{summary} · {duration_ms}ms"),
        None => summary,
    }
}

pub(super) fn value_map(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

pub(super) fn runtime_role_label(role: RuntimeRole) -> &'static str {
    match role {
        RuntimeRole::Worker => "worker",
        RuntimeRole::Validator => "validator",
    }
}

pub(super) fn runtime_stream_label(stream: RuntimeOutputStream) -> &'static str {
    match stream {
        RuntimeOutputStream::Stdout => "stdout",
        RuntimeOutputStream::Stderr => "stderr",
    }
}
