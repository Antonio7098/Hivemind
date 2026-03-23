use super::*;
use crate::cli::handlers::attempt::runtime_data::AttemptInspectCollected;
use crate::cli::handlers::attempt::AttemptInspectArgs;
use crate::core::events::CorrelationIds;
use uuid::Uuid;

pub(crate) fn build_attempt_inspect_json(
    attempt_id: Uuid,
    corr: &CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &AttemptInspectArgs,
) -> serde_json::Value {
    let mut info = serde_json::Map::new();

    info.insert(
        "attempt_id".to_string(),
        serde_json::Value::String(attempt_id.to_string()),
    );
    if let Some(pid) = corr.project_id {
        info.insert(
            "project_id".to_string(),
            serde_json::Value::String(pid.to_string()),
        );
    }
    if let Some(gid) = corr.graph_id {
        info.insert(
            "graph_id".to_string(),
            serde_json::Value::String(gid.to_string()),
        );
    }
    if let Some(fid) = corr.flow_id {
        info.insert(
            "flow_id".to_string(),
            serde_json::Value::String(fid.to_string()),
        );
    }
    if let Some(tid) = collected.task_id.or(corr.task_id) {
        info.insert(
            "task_id".to_string(),
            serde_json::Value::String(tid.to_string()),
        );
    }
    if let Some(an) = collected.adapter_name.clone() {
        info.insert("adapter_name".to_string(), serde_json::Value::String(an));
    }
    if let Some(ec) = collected.exit_code {
        info.insert(
            "exit_code".to_string(),
            serde_json::Value::Number(ec.into()),
        );
    }
    if let Some(dm) = collected.duration_ms {
        info.insert(
            "duration_ms".to_string(),
            serde_json::Value::Number(dm.into()),
        );
    }
    if let Some(reason) = collected.terminated_reason.clone() {
        info.insert(
            "terminated_reason".to_string(),
            serde_json::Value::String(reason),
        );
    }

    if args.output {
        info.insert(
            "stdout".to_string(),
            serde_json::Value::String(collected.stdout.clone()),
        );
        info.insert(
            "stderr".to_string(),
            serde_json::Value::String(collected.stderr.clone()),
        );
    }
    if args.diff {
        info.insert(
            "files_created".to_string(),
            serde_json::to_value(&collected.files_created).unwrap_or(serde_json::Value::Null),
        );
        info.insert(
            "files_modified".to_string(),
            serde_json::to_value(&collected.files_modified).unwrap_or(serde_json::Value::Null),
        );
        info.insert(
            "files_deleted".to_string(),
            serde_json::to_value(&collected.files_deleted).unwrap_or(serde_json::Value::Null),
        );
    }
    if args.context {
        info.insert(
            "context".to_string(),
            serde_json::json!({
                "retry": collected.retry_context.clone(),
                "manifest": collected.context_manifest.clone(),
                "context_window_hash": collected.context_window_state_hash.clone(),
                "manifest_hash": collected.context_manifest_hash.clone(),
                "inputs_hash": collected.context_inputs_hash.clone(),
                "rendered_prompt_hash": collected.rendered_prompt_hash.clone(),
                "delivered_context_hash": collected.delivered_context_hash.clone(),
            }),
        );
    }

    serde_json::Value::Object(info)
}
