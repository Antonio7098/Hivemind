use super::*;
use crate::core::events::CorrelationIds;

pub(crate) fn print_attempt_inspect_attempt(
    registry: &Registry,
    attempt: &AttemptState,
    show_diff: bool,
    show_context: bool,
    format: OutputFormat,
) -> ExitCode {
    let diff = if show_diff {
        match registry.get_attempt_diff(&attempt.id.to_string()) {
            Ok(d) => d,
            Err(e) => return output_error(&e, format),
        }
    } else {
        None
    };
    let context_value = if show_context {
        attempt_context_from_events(registry, attempt.id)
    } else {
        None
    };

    let info = serde_json::json!({
        "attempt_id": attempt.id,
        "task_id": attempt.task_id,
        "flow_id": attempt.flow_id,
        "attempt_number": attempt.attempt_number,
        "started_at": attempt.started_at,
        "baseline_id": attempt.baseline_id,
        "diff_id": attempt.diff_id,
        "runtime_session": attempt.runtime_session,
        "turn_refs": attempt.turn_refs,
        "diff": diff,
        "context": context_value,
    });

    match format {
        OutputFormat::Json => {
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Attempt:  {}", attempt.id);
            println!("Task:     {}", attempt.task_id);
            println!("Flow:     {}", attempt.flow_id);
            println!("Number:   {}", attempt.attempt_number);
            println!("Started:  {}", attempt.started_at);
            if let Some(b) = attempt.baseline_id {
                println!("Baseline: {b}");
            }
            if let Some(did) = attempt.diff_id {
                println!("Diff:     {did}");
            }
            if let Some(ctx) = context_value {
                if let Ok(rendered) = serde_json::to_string_pretty(&ctx) {
                    println!("Context:\n{rendered}");
                }
            }
            if let Some(session) = attempt.runtime_session.as_ref() {
                println!(
                    "Runtime:  {} session {} ({})",
                    session.adapter_name, session.session_id, session.discovered_at
                );
            }
            if !attempt.turn_refs.is_empty() {
                println!("Turn refs:");
                for turn in &attempt.turn_refs {
                    println!(
                        "  - turn {} [{}] ref={} commit={}",
                        turn.ordinal,
                        format!("{:?}", turn.stream).to_lowercase(),
                        turn.git_ref.as_deref().unwrap_or("-"),
                        turn.commit_sha.as_deref().unwrap_or("-")
                    );
                }
            }
            if let Some(d) = diff {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}

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

pub(crate) fn print_attempt_inspect_table(
    attempt_id: Uuid,
    corr: &CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &AttemptInspectArgs,
) {
    println!("Attempt:  {attempt_id}");
    if let Some(fid) = corr.flow_id {
        println!("Flow:     {fid}");
    }
    if let Some(tid) = collected.task_id.or(corr.task_id) {
        println!("Task:     {tid}");
    }
    if let Some(an) = collected.adapter_name.as_ref() {
        println!("Adapter:  {an}");
    }
    if let Some(ec) = collected.exit_code {
        println!("Exit:     {ec}");
    }
    if let Some(dm) = collected.duration_ms {
        println!("Duration: {dm}ms");
    }
    if let Some(reason) = collected.terminated_reason.as_ref() {
        println!("Terminated: {reason}");
    }
    if args.diff {
        println!("Changes:");
        if !collected.files_created.is_empty() {
            println!("  Created:");
            for p in &collected.files_created {
                println!("    - {}", p.display());
            }
        }
        if !collected.files_modified.is_empty() {
            println!("  Modified:");
            for p in &collected.files_modified {
                println!("    - {}", p.display());
            }
        }
        if !collected.files_deleted.is_empty() {
            println!("  Deleted:");
            for p in &collected.files_deleted {
                println!("    - {}", p.display());
            }
        }
    }
    if args.output {
        println!("Stdout:\n{}", collected.stdout);
        println!("Stderr:\n{}", collected.stderr);
    }
    if args.context {
        println!("Context:");
        if let Some(hash) = collected.context_manifest_hash.as_ref() {
            println!("  Manifest hash: {hash}");
        }
        if let Some(hash) = collected.context_inputs_hash.as_ref() {
            println!("  Inputs hash:   {hash}");
        }
        if let Some(hash) = collected.context_window_state_hash.as_ref() {
            println!("  Window hash:   {hash}");
        }
        if let Some(hash) = collected.rendered_prompt_hash.as_ref() {
            println!("  Prompt hash:   {hash}");
        }
        if let Some(hash) = collected.delivered_context_hash.as_ref() {
            println!("  Delivered hash:{hash}");
        }
        if let Some(ctx) = collected.retry_context.as_ref() {
            println!("  Retry:\n{ctx}");
        } else {
            println!("  Retry: (none)");
        }
        if let Some(manifest) = collected.context_manifest.as_ref() {
            if let Ok(rendered) = serde_json::to_string_pretty(manifest) {
                println!("  Manifest:\n{rendered}");
            }
        } else {
            println!("  Manifest: (none)");
        }
    }
}
