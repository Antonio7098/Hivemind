use super::*;

pub(crate) struct AttemptInspectCollected {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) adapter_name: Option<String>,
    pub(crate) task_id: Option<Uuid>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) terminated_reason: Option<String>,
    pub(crate) files_created: Vec<std::path::PathBuf>,
    pub(crate) files_modified: Vec<std::path::PathBuf>,
    pub(crate) files_deleted: Vec<std::path::PathBuf>,
    pub(crate) retry_context: Option<String>,
    pub(crate) context_manifest: Option<serde_json::Value>,
    pub(crate) context_manifest_hash: Option<String>,
    pub(crate) context_inputs_hash: Option<String>,
    pub(crate) context_window_state_hash: Option<String>,
    pub(crate) rendered_prompt_hash: Option<String>,
    pub(crate) delivered_context_hash: Option<String>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn collect_attempt_runtime_data(
    events: &[crate::core::events::Event],
) -> AttemptInspectCollected {
    use crate::core::events::{EventPayload, RuntimeOutputStream};

    let mut collected = AttemptInspectCollected {
        stdout: String::new(),
        stderr: String::new(),
        adapter_name: None,
        task_id: None,
        exit_code: None,
        duration_ms: None,
        terminated_reason: None,
        files_created: Vec::new(),
        files_modified: Vec::new(),
        files_deleted: Vec::new(),
        retry_context: None,
        context_manifest: None,
        context_manifest_hash: None,
        context_inputs_hash: None,
        context_window_state_hash: None,
        rendered_prompt_hash: None,
        delivered_context_hash: None,
    };

    for ev in events {
        match &ev.payload {
            EventPayload::RuntimeStarted {
                adapter_name,
                task_id,
                ..
            } => {
                collected.adapter_name = Some(adapter_name.clone());
                collected.task_id = Some(*task_id);
            }
            EventPayload::RuntimeOutputChunk {
                stream, content, ..
            } => match stream {
                RuntimeOutputStream::Stdout => {
                    collected.stdout.push_str(content);
                    collected.stdout.push('\n');
                }
                RuntimeOutputStream::Stderr => {
                    collected.stderr.push_str(content);
                    collected.stderr.push('\n');
                }
            },
            EventPayload::RuntimeExited {
                exit_code,
                duration_ms,
                ..
            } => {
                collected.exit_code = Some(*exit_code);
                collected.duration_ms = Some(*duration_ms);
            }
            EventPayload::RuntimeTerminated { reason, .. } => {
                collected.terminated_reason = Some(reason.clone());
            }
            EventPayload::RuntimeFilesystemObserved {
                files_created,
                files_modified,
                files_deleted,
                ..
            } => {
                collected.files_created.clone_from(files_created);
                collected.files_modified.clone_from(files_modified);
                collected.files_deleted.clone_from(files_deleted);
            }
            EventPayload::RetryContextAssembled { context, .. } => {
                collected.retry_context = Some(context.clone());
            }
            EventPayload::AttemptContextAssembled {
                manifest_hash,
                inputs_hash,
                context_hash,
                manifest_json,
                ..
            } => {
                collected.context_manifest_hash = Some(manifest_hash.clone());
                collected.context_inputs_hash = Some(inputs_hash.clone());
                collected.rendered_prompt_hash = Some(context_hash.clone());
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(manifest_json) {
                    collected.context_manifest = Some(parsed);
                } else {
                    collected.context_manifest =
                        Some(serde_json::Value::String(manifest_json.clone()));
                }
            }
            EventPayload::ContextWindowCreated { state_hash, .. } => {
                collected.context_window_state_hash = Some(state_hash.clone());
            }
            EventPayload::ContextWindowSnapshotCreated {
                rendered_prompt_hash,
                delivered_input_hash,
                ..
            } => {
                collected.rendered_prompt_hash = Some(rendered_prompt_hash.clone());
                collected.delivered_context_hash = Some(delivered_input_hash.clone());
            }
            EventPayload::AttemptContextDelivered { context_hash, .. } => {
                collected.delivered_context_hash = Some(context_hash.clone());
            }
            _ => {}
        }
    }

    collected
}

pub(crate) fn attempt_context_from_events(
    registry: &Registry,
    attempt_id: Uuid,
) -> Option<serde_json::Value> {
    use crate::core::events::EventPayload;
    use crate::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.attempt_id = Some(attempt_id);
    let events = registry.read_events(&filter).ok()?;
    let mut retry_context: Option<String> = None;
    let mut manifest: Option<serde_json::Value> = None;
    let mut context_window_hash: Option<String> = None;
    let mut manifest_hash: Option<String> = None;
    let mut inputs_hash: Option<String> = None;
    let mut rendered_prompt_hash: Option<String> = None;
    let mut delivered_context_hash: Option<String> = None;

    for event in events {
        match event.payload {
            EventPayload::RetryContextAssembled { context, .. } => retry_context = Some(context),
            EventPayload::ContextWindowCreated { state_hash, .. } => {
                context_window_hash = Some(state_hash)
            }
            EventPayload::ContextWindowSnapshotCreated {
                rendered_prompt_hash: hash,
                delivered_input_hash,
                ..
            } => {
                rendered_prompt_hash = Some(hash);
                delivered_context_hash = Some(delivered_input_hash);
            }
            EventPayload::AttemptContextAssembled {
                manifest_hash: hash,
                inputs_hash: in_hash,
                context_hash,
                manifest_json,
                ..
            } => {
                manifest_hash = Some(hash);
                inputs_hash = Some(in_hash);
                rendered_prompt_hash = Some(context_hash);
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&manifest_json) {
                    manifest = Some(parsed);
                } else {
                    manifest = Some(serde_json::Value::String(manifest_json));
                }
            }
            EventPayload::AttemptContextDelivered { context_hash, .. } => {
                delivered_context_hash = Some(context_hash);
            }
            _ => {}
        }
    }

    if retry_context.is_none() && manifest.is_none() {
        None
    } else {
        Some(serde_json::json!({
            "retry": retry_context,
            "manifest": manifest,
            "context_window_hash": context_window_hash,
            "manifest_hash": manifest_hash,
            "inputs_hash": inputs_hash,
            "rendered_prompt_hash": rendered_prompt_hash,
            "delivered_context_hash": delivered_context_hash,
        }))
    }
}
