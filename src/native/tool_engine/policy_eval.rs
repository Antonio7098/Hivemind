use super::*;
mod approval;
mod dangerous;
mod network;
use approval::evaluate_approval_and_exec_policy;
use dangerous::*;
use network::evaluate_network_targets;

// ARCH_DEBT: legacy oversized function
#[allow(clippy::too_many_lines)]
pub(super) fn evaluate_tool_policies_impl(
    action: &NativeToolAction,
    tool: &RegisteredTool,
    ctx: &ToolExecutionContext<'_>,
) -> Result<Vec<String>, NativeToolEngineError> {
    let mut tags = ctx.sandbox_policy.base_policy_tags();
    tags.push(format!(
        "approval_mode:{}",
        ctx.approval_policy.mode.as_policy_value()
    ));
    let requires_write = tool
        .contract
        .required_permissions
        .iter()
        .any(|perm| matches!(perm, ToolPermission::FilesystemWrite));
    let requires_exec = tool
        .contract
        .required_permissions
        .iter()
        .any(|perm| matches!(perm, ToolPermission::Execution));
    match ctx.sandbox_policy.mode {
        NativeSandboxMode::ReadOnly if requires_write || requires_exec => {
            tags.push("sandbox_decision:denied".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "tool '{}' denied by sandbox policy '{}' (write/exec blocked)",
                action.name,
                ctx.sandbox_policy.mode.as_policy_value()
            ))
            .with_policy_tags(tags));
        }
        NativeSandboxMode::WorkspaceWrite if action.name == "write_file" => {
            let write = decode_input::<WriteFileInput>(&action.input)?;
            let rel = normalize_relative_path(&write.path, false)?;
            let rel_display = relative_display(&rel);
            let roots = if ctx.sandbox_policy.writable_roots.is_empty() {
                vec![".".to_string()]
            } else {
                ctx.sandbox_policy.writable_roots.clone()
            };
            let allowed = roots.iter().any(|root| {
                root == "." || rel_display == *root || rel_display.starts_with(&format!("{root}/"))
            });
            if !allowed {
                tags.push("sandbox_decision:denied".to_string());
                return Err(NativeToolEngineError::policy_violation(format!("tool 'write_file' denied by sandbox policy '{}': path '{}' is outside writable roots [{}]", ctx.sandbox_policy.mode.as_policy_value(), rel_display, roots.join(", "))).with_policy_tags(tags));
            }
            tags.push("sandbox_decision:workspace_write_allow".to_string());
        }
        _ => tags.push("sandbox_decision:allowed".to_string()),
    }
    let mut approval_cache_key = format!("tool:{}", action.name);
    let mut command_line = None;
    let mut fallback_command_line = None;
    let mut dangerous_reason = None;
    if action.name == "run_command" || action.name == "exec_command" {
        let (raw_command, args) = if action.name == "run_command" {
            let run = decode_input::<RunCommandInput>(&action.input)?;
            (run.command.trim().to_string(), run.args)
        } else {
            let run = decode_input::<ExecCommandInput>(&action.input)?;
            (run.cmd.trim().to_string(), run.args)
        };
        let command = normalize_exec_command(&raw_command, ctx.env);
        let joined = if args.is_empty() {
            command.clone()
        } else {
            format!("{command} {}", args.join(" "))
        };
        if args.is_empty() {
            if raw_command != joined {
                fallback_command_line = Some(raw_command);
            }
        } else {
            let raw_joined = format!("{raw_command} {}", args.join(" "));
            if raw_joined != joined {
                fallback_command_line = Some(raw_joined);
            }
        }
        dangerous_reason = dangerous_command_reason(&command, &args);
        approval_cache_key = command.to_ascii_lowercase();
        command_line = Some(joined);
        let network_targets = extract_network_targets(&command, &args);
        evaluate_network_targets(ctx, &network_targets, &mut tags)?;
    }
    evaluate_approval_and_exec_policy(
        ctx,
        requires_write,
        requires_exec,
        &approval_cache_key,
        command_line.as_deref(),
        fallback_command_line.as_deref(),
        dangerous_reason,
        &mut tags,
    )?;
    Ok(tags)
}
