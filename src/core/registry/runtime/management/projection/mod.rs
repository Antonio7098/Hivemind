#![allow(
    clippy::implicit_clone,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]

use super::*;
use serde_json::{json, Map, Value};

mod approval;
mod stream;

use approval::{
    approval_projection_from_events, approval_stream_title, completed_approval_items,
    failed_approval_items, requested_approvals_from_policy_tags, take_pending_approvals,
};
pub(crate) use stream::{
    runtime_stream_items_from_events, runtime_stream_items_from_events_with_detail,
};

enum RuntimeSessionSource {
    Fallback,
    Invocation,
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

fn parse_projection_uuid(raw: &str, code: &'static str, origin: &'static str) -> Result<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| HivemindError::user(code, format!("'{raw}' is not a valid UUID"), origin))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence: u64, correlation: CorrelationIds, payload: EventPayload) -> Event {
        let mut event = Event::new(payload, correlation);
        event.metadata.sequence = Some(sequence);
        event
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
        let correlation = CorrelationIds::for_graph_flow_task_attempt(
            project_id, graph_id, flow_id, task_id, attempt_id,
        );
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
                    active_code_window_trace: vec![],
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
        let correlation = CorrelationIds::for_graph_flow_task_attempt(
            project_id, graph_id, flow_id, task_id, attempt_id,
        );
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
        let correlation = CorrelationIds::for_graph_flow_task_attempt(
            project_id, graph_id, flow_id, task_id, attempt_id,
        );
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
        let correlation = CorrelationIds::for_graph_flow_task_attempt(
            project_id, graph_id, flow_id, task_id, attempt_id,
        );
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
        let correlation = CorrelationIds::for_graph_flow_task_attempt(
            project_id, graph_id, flow_id, task_id, attempt_id,
        );
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
