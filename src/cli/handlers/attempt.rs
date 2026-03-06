//! Attempt command handlers.

use crate::cli::commands::{AttemptCommands, AttemptInspectArgs};
use crate::cli::handlers::common::get_registry;
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;
use crate::core::state::AttemptState;
use uuid::Uuid;
pub fn handle_attempt(cmd: AttemptCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        AttemptCommands::List(args) => {
            match registry.list_attempts(args.flow.as_deref(), args.task.as_deref(), args.limit) {
                Ok(attempts) => match format {
                    OutputFormat::Table => {
                        if attempts.is_empty() {
                            println!("No attempts found.");
                        } else {
                            println!(
                                "{:<36}  {:<36}  {:<36}  {:<8}  CHECKPOINTS",
                                "ATTEMPT", "FLOW", "TASK", "NUMBER"
                            );
                            println!("{}", "-".repeat(170));
                            for attempt in attempts {
                                println!(
                                    "{:<36}  {:<36}  {:<36}  {:<8}  {}",
                                    attempt.attempt_id,
                                    attempt.flow_id,
                                    attempt.task_id,
                                    attempt.attempt_number,
                                    if attempt.all_checkpoints_completed {
                                        "all_completed"
                                    } else {
                                        "incomplete"
                                    }
                                );
                            }
                        }
                        ExitCode::Success
                    }
                    _ => output(&attempts, format)
                        .map(|()| ExitCode::Success)
                        .unwrap_or(ExitCode::Error),
                },
                Err(e) => output_error(&e, format),
            }
        }
        AttemptCommands::Inspect(args) => handle_attempt_inspect(&registry, &args, format),
    }
}

pub fn handle_attempt_inspect(
    registry: &Registry,
    args: &AttemptInspectArgs,
    format: OutputFormat,
) -> ExitCode {
    let attempt_id = &args.attempt_id;
    let Ok(parsed) = Uuid::parse_str(attempt_id) else {
        return output_error(
            &crate::core::error::HivemindError::user(
                "invalid_attempt_id",
                format!("'{attempt_id}' is not a valid attempt ID"),
                "cli:attempt:inspect",
            ),
            format,
        );
    };

    match try_print_attempt_from_events(registry, parsed, args, format) {
        Ok(Some(code)) => return code,
        Ok(None) => {}
        Err(e) => return output_error(&e, format),
    }

    match registry.get_attempt(attempt_id) {
        Ok(attempt) => {
            print_attempt_inspect_attempt(registry, &attempt, args.diff, args.context, format)
        }
        Err(e) if e.code == "attempt_not_found" => {
            print_attempt_inspect_task_fallback(registry, parsed, args.diff, format, &e)
        }
        Err(e) => output_error(&e, format),
    }
}

fn print_attempt_inspect_attempt(
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
            if let Some(d) = diff {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}

struct AttemptInspectCollected {
    stdout: String,
    stderr: String,
    adapter_name: Option<String>,
    task_id: Option<Uuid>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    terminated_reason: Option<String>,
    files_created: Vec<std::path::PathBuf>,
    files_modified: Vec<std::path::PathBuf>,
    files_deleted: Vec<std::path::PathBuf>,
    retry_context: Option<String>,
    context_manifest: Option<serde_json::Value>,
    context_manifest_hash: Option<String>,
    context_inputs_hash: Option<String>,
    context_window_state_hash: Option<String>,
    rendered_prompt_hash: Option<String>,
    delivered_context_hash: Option<String>,
}

#[allow(clippy::too_many_lines)]
fn collect_attempt_runtime_data(events: &[crate::core::events::Event]) -> AttemptInspectCollected {
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
                attempt_id: _,
                stream,
                content,
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
                attempt_id: _,
                exit_code,
                duration_ms,
            } => {
                collected.exit_code = Some(*exit_code);
                collected.duration_ms = Some(*duration_ms);
            }
            EventPayload::RuntimeTerminated {
                attempt_id: _,
                reason,
            } => {
                collected.terminated_reason = Some(reason.clone());
            }
            EventPayload::RuntimeFilesystemObserved {
                attempt_id: _,
                files_created,
                files_modified,
                files_deleted,
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

fn build_attempt_inspect_json(
    attempt_id: Uuid,
    corr: &crate::core::events::CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &crate::cli::commands::AttemptInspectArgs,
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

fn print_attempt_inspect_table(
    attempt_id: Uuid,
    corr: &crate::core::events::CorrelationIds,
    collected: &AttemptInspectCollected,
    args: &crate::cli::commands::AttemptInspectArgs,
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

fn attempt_context_from_events(registry: &Registry, attempt_id: Uuid) -> Option<serde_json::Value> {
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
            EventPayload::RetryContextAssembled { context, .. } => {
                retry_context = Some(context);
            }
            EventPayload::ContextWindowCreated { state_hash, .. } => {
                context_window_hash = Some(state_hash);
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

fn try_print_attempt_from_events(
    registry: &Registry,
    attempt_id: Uuid,
    args: &crate::cli::commands::AttemptInspectArgs,
    format: OutputFormat,
) -> Result<Option<ExitCode>, crate::core::error::HivemindError> {
    use crate::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.attempt_id = Some(attempt_id);
    let events = registry.read_events(&filter)?;
    if events.is_empty() {
        return Ok(None);
    }

    let corr = events[0].metadata.correlation.clone();

    let collected = collect_attempt_runtime_data(&events);
    let info = build_attempt_inspect_json(attempt_id, &corr, &collected, args);
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
            print_attempt_inspect_table(attempt_id, &corr, &collected, args);

            if args.diff {
                if let Some(diff) = registry.get_attempt_diff(&attempt_id.to_string())? {
                    println!("{diff}");
                }
            }
        }
    }

    Ok(Some(ExitCode::Success))
}

fn print_attempt_inspect_task_fallback(
    registry: &Registry,
    task_id: Uuid,
    show_diff: bool,
    format: OutputFormat,
    original_error: &crate::core::error::HivemindError,
) -> ExitCode {
    let state = match registry.state() {
        Ok(s) => s,
        Err(e) => return output_error(&e, format),
    };

    let exec_info = state
        .flows
        .values()
        .find_map(|flow| flow.task_executions.get(&task_id).map(|exec| (flow, exec)));

    let Some((flow, exec)) = exec_info else {
        return output_error(original_error, format);
    };

    let latest_attempt = state
        .attempts
        .values()
        .filter(|a| a.task_id == task_id && a.flow_id == flow.id)
        .max_by_key(|a| a.started_at);

    let diff = if show_diff {
        latest_attempt
            .and_then(|a| a.diff_id.map(|_| a.id))
            .and_then(|aid| registry.get_attempt_diff(&aid.to_string()).ok())
            .flatten()
    } else {
        None
    };

    let info = serde_json::json!({
        "task_id": exec.task_id,
        "state": format!("{:?}", exec.state),
        "attempt_count": exec.attempt_count,
        "blocked_reason": exec.blocked_reason,
        "flow_id": flow.id,
        "diff": diff,
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
            println!("Task:     {}", exec.task_id);
            println!("Flow:     {}", flow.id);
            println!("State:    {:?}", exec.state);
            println!("Attempts: {}", exec.attempt_count);
            if let Some(ref reason) = exec.blocked_reason {
                println!("Blocked:  {reason}");
            }
            if let Some(d) = info.get("diff").and_then(|v| v.as_str()) {
                println!("{d}");
            }
        }
    }

    ExitCode::Success
}
