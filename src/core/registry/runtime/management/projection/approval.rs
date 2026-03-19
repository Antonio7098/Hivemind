use super::stream::{value_map, with_duration};
use super::*;
use chrono::DateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalKind {
    Execution,
    Network,
}

#[derive(Debug, Clone)]
struct ApprovalProjectionState {
    view: RuntimeApprovalView,
}

pub(super) fn approval_projection_from_events(events: &[Event]) -> Vec<RuntimeApprovalView> {
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
            } => resolve_pending_approvals(
                &mut approvals,
                call_id,
                policy_tags,
                event.metadata.timestamp,
                None,
                *duration_ms,
                false,
            ),
            EventPayload::ToolCallFailed {
                call_id,
                message,
                denial_reason,
                duration_ms,
                policy_tags,
                ..
            } => resolve_pending_approvals(
                &mut approvals,
                call_id,
                policy_tags,
                event.metadata.timestamp,
                denial_reason.as_deref().or(Some(message.as_str())),
                *duration_ms,
                true,
            ),
            _ => {}
        }
    }
    let mut views: Vec<_> = approvals.into_values().map(|state| state.view).collect();
    views.sort_by_key(|approval| approval.requested_at);
    views
}

pub(super) fn completed_approval_items(
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
            text: approval_summary(tool_name, kind, policy_tags, None).map(|summary| with_duration(summary, duration_ms)),
            data: value_map(json!({"approval_id": resolved_approval.approval_id, "call_id": resolved_approval.call_id, "invocation_id": resolved_approval.invocation_id, "turn_index": resolved_approval.turn_index, "tool_name": resolved_approval.tool_name, "approval_kind": resolved_approval.approval_kind, "status": resolved_approval.status, "resource": resolved_approval.resource, "decision": resolved_approval.decision, "policy_tags": policy_tags})),
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
        text: Some(with_duration(format!("turn {} · {call_id}", turn_index + 1), duration_ms)),
        data: value_map(json!({"call_id": call_id, "invocation_id": invocation_id, "turn_index": turn_index, "tool_name": tool_name, "duration_ms": duration_ms, "policy_tags": policy_tags})),
    });
    items
}

pub(super) fn failed_approval_items(
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
            text: approval_summary(tool_name, kind, policy_tags, denial_reason.or(Some(message))),
            data: value_map(json!({"approval_id": resolved_approval.approval_id, "call_id": resolved_approval.call_id, "invocation_id": resolved_approval.invocation_id, "turn_index": resolved_approval.turn_index, "tool_name": resolved_approval.tool_name, "approval_kind": resolved_approval.approval_kind, "status": resolved_approval.status, "resource": resolved_approval.resource, "decision": resolved_approval.decision, "policy_tags": policy_tags})),
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
        text: Some(with_duration(denial_reason.unwrap_or(message).to_string(), duration_ms)),
        data: value_map(json!({"call_id": call_id, "invocation_id": invocation_id, "turn_index": turn_index, "tool_name": tool_name, "duration_ms": duration_ms, "message": message, "denial_reason": denial_reason, "policy_tags": policy_tags})),
    });
    items
}

pub(super) fn take_pending_approvals(
    pending_approvals: &mut HashMap<String, Vec<RuntimeApprovalView>>,
    call_id: &str,
) -> Vec<RuntimeApprovalView> {
    pending_approvals.remove(call_id).unwrap_or_default()
}

pub(super) fn requested_approvals_from_policy_tags(
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
pub(super) fn approval_stream_title(approval: &RuntimeApprovalView) -> String {
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
            Some(match execution_approval_status(policy_tags) {
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
            Some(match network_approval_status(policy_tags) {
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
