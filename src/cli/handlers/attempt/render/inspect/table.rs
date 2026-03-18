use super::*;
use crate::cli::handlers::attempt::runtime_data::AttemptInspectCollected;
use crate::cli::handlers::attempt::AttemptInspectArgs;
use crate::core::events::CorrelationIds;
use uuid::Uuid;

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
