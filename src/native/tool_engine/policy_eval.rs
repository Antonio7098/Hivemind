use super::*;
mod dangerous;
use dangerous::*;

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
    let mut approval_required = false;
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
        tags.extend(ctx.network_policy.base_policy_tags());
        let (_, http_clamped) = clamp_bind_address(
            &ctx.network_policy.proxy_http_bind,
            ctx.network_policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (_, admin_clamped) = clamp_bind_address(
            &ctx.network_policy.proxy_admin_bind,
            ctx.network_policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (_, socks_clamped) = clamp_bind_address(
            &ctx.network_policy.proxy_socks5_bind,
            ctx.network_policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        tags.push(format!(
            "network_proxy_bind_clamped:{}",
            http_clamped || admin_clamped || socks_clamped
        ));
        if network_targets.is_empty() {
            tags.push("network_targets:none".to_string());
        } else {
            tags.push(format!("network_targets_count:{}", network_targets.len()));
            for target in &network_targets {
                tags.push(format!(
                    "network_target:{}",
                    format_network_target_tag_value(target)
                ));
                if matches!(
                    ctx.network_policy.access_mode,
                    NativeNetworkAccessMode::Disabled
                ) {
                    tags.push("network_decision:denied_mode_disabled".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "network policy denied '{}' because network mode is disabled",
                        target.cache_key()
                    ))
                    .with_policy_tags(tags));
                }
                if ctx
                    .network_policy
                    .denylist
                    .iter()
                    .any(|pattern| matches_host_pattern(pattern, &target.host))
                {
                    tags.push("network_decision:denied_denylist".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "network policy denied '{}' by denylist",
                        target.cache_key()
                    ))
                    .with_policy_tags(tags));
                }
                if !ctx.network_policy.allowlist.is_empty()
                    && !ctx
                        .network_policy
                        .allowlist
                        .iter()
                        .any(|pattern| matches_host_pattern(pattern, &target.host))
                {
                    tags.push("network_decision:denied_not_allowlisted".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "network policy denied '{}': host is not in allowlist",
                        target.cache_key()
                    ))
                    .with_policy_tags(tags));
                }
                if ctx.network_policy.block_private_addresses
                    && is_private_or_local_host(&target.host)
                {
                    tags.push("network_decision:denied_private_address".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "network policy denied '{}': private/local address blocked",
                        target.cache_key()
                    ))
                    .with_policy_tags(tags));
                }
                if matches!(
                    ctx.network_policy.access_mode,
                    NativeNetworkAccessMode::Limited
                ) && !ctx
                    .network_policy
                    .limited_methods
                    .iter()
                    .any(|method| method.eq_ignore_ascii_case(&target.method))
                {
                    tags.push("network_decision:denied_method_restricted".to_string());
                    return Err(NativeToolEngineError::policy_violation(format!(
                        "network policy denied '{}': method '{}' is not allowed in limited mode",
                        target.cache_key(),
                        target.method
                    ))
                    .with_policy_tags(tags));
                }
                let approval_key = target.cache_key();
                match ctx.network_policy.approval_mode {
                    NativeNetworkApprovalMode::None => {
                        tags.push("network_approval_required:false".to_string());
                        tags.push("network_approval_outcome:not_required".to_string());
                    }
                    NativeNetworkApprovalMode::Immediate => {
                        tags.push("network_approval_required:true".to_string());
                        let mut cache = ctx.network_approval_cache.borrow_mut();
                        if cache.contains(&approval_key) {
                            tags.push("network_approval_outcome:approved_cached".to_string());
                        } else if matches!(
                            ctx.network_policy.approval_decision,
                            NativeNetworkApprovalDecision::Approve
                        ) {
                            cache.insert_bounded(
                                approval_key,
                                ctx.network_policy.approval_cache_max_entries,
                            );
                            tags.push("network_approval_outcome:approved_for_session".to_string());
                        } else {
                            tags.push("network_approval_outcome:denied".to_string());
                            return Err(NativeToolEngineError::policy_violation(format!(
                                "network approval denied '{}'",
                                target.cache_key()
                            ))
                            .with_policy_tags(tags));
                        }
                    }
                    NativeNetworkApprovalMode::Deferred => {
                        tags.push("network_approval_required:true".to_string());
                        if ctx.network_policy.deferred_decisions_file.is_none() {
                            tags.push("network_approval_outcome:denied_no_watcher".to_string());
                            return Err(NativeToolEngineError::policy_violation(format!("network approval deferred mode requires {NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE_ENV_KEY}")).with_policy_tags(tags));
                        }
                        tags.push("network_approval_outcome:deferred_pending".to_string());
                    }
                }
            }
            tags.push("network_decision:preflight_allowed".to_string());
        }
        if matches!(
            ctx.network_policy.proxy_mode,
            NativeNetworkProxyMode::Managed
        ) {
            tags.push("network_proxy:managed".to_string());
        } else {
            tags.push("network_proxy:off".to_string());
        }
    }
    let trusted = command_line.as_deref().is_some_and(|line| {
        ctx.approval_policy
            .trusted_prefixes
            .iter()
            .any(|prefix| matches_command_pattern(prefix, line))
    }) || fallback_command_line.as_deref().is_some_and(|line| {
        ctx.approval_policy
            .trusted_prefixes
            .iter()
            .any(|prefix| matches_command_pattern(prefix, line))
    });
    match ctx.approval_policy.mode {
        NativeApprovalMode::Never => {
            tags.push("approval_required:false".to_string());
            tags.push("approval_outcome:not_required".to_string());
        }
        NativeApprovalMode::OnFailure => {
            tags.push("approval_required:false".to_string());
            tags.push("approval_outcome:deferred_on_failure".to_string());
        }
        NativeApprovalMode::OnRequest => approval_required = requires_write || requires_exec,
        NativeApprovalMode::UnlessTrusted => {
            approval_required = (requires_write || requires_exec) && !trusted;
            tags.push(format!("approval_trusted_prefix:{trusted}"));
        }
    }
    if dangerous_reason.is_some() {
        approval_required = true;
        tags.push("exec_dangerous:true".to_string());
    } else {
        tags.push("exec_dangerous:false".to_string());
    }
    if approval_required {
        tags.push("approval_required:true".to_string());
        if is_broad_prefix(&approval_cache_key) {
            tags.push("approval_outcome:denied_broad_prefix".to_string());
            return Err(NativeToolEngineError::policy_violation(format!("approval denied for broad prefix '{approval_cache_key}'; provide a specific command prefix")).with_policy_tags(tags));
        }
        let mut cache = ctx.approval_cache.borrow_mut();
        if cache.contains(&approval_cache_key) {
            tags.push("approval_review_decision:cached".to_string());
            tags.push("approval_outcome:approved_for_session".to_string());
            tags.push("approved_for_session:true".to_string());
        } else if ctx.approval_policy.review_decision == NativeApprovalReviewDecision::Approve {
            cache.insert_bounded(approval_cache_key, ctx.approval_policy.cache_max_entries);
            tags.push("approval_review_decision:approve".to_string());
            tags.push("approval_outcome:approved_for_session".to_string());
            tags.push("approved_for_session:true".to_string());
        } else {
            tags.push("approval_review_decision:deny".to_string());
            tags.push("approval_outcome:denied".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "approval denied for '{}' under mode '{}'",
                approval_cache_key,
                ctx.approval_policy.mode.as_policy_value()
            ))
            .with_policy_tags(tags));
        }
    }
    if let Some(reason) = dangerous_reason {
        tags.push(format!(
            "exec_danger_reason:{}",
            sanitize_policy_tag_value(&reason)
        ));
        if !matches!(
            ctx.sandbox_policy.mode,
            NativeSandboxMode::DangerFullAccess | NativeSandboxMode::HostPassthrough
        ) {
            tags.push("exec_decision:denied_dangerous_requires_elevation".to_string());
            return Err(NativeToolEngineError::policy_violation(format!("dangerous command denied: {reason}. Set {SANDBOX_MODE_ENV_KEY}=danger-full-access (or host-passthrough) and request approval.")).with_policy_tags(tags));
        }
        if matches!(
            ctx.approval_policy.mode,
            NativeApprovalMode::Never | NativeApprovalMode::OnFailure
        ) {
            tags.push("exec_decision:denied_dangerous_requires_preapproval".to_string());
            return Err(NativeToolEngineError::policy_violation(format!("dangerous command denied: {reason}. Set {APPROVAL_MODE_ENV_KEY} to on-request or unless-trusted.")).with_policy_tags(tags));
        }
    }
    if let Some(line) = command_line.as_deref() {
        let cache = ctx.approval_cache.borrow();
        let allowed_primary = ctx.exec_policy_manager.is_allowed(line, &cache);
        let allowed_fallback = fallback_command_line
            .as_deref()
            .is_some_and(|fallback| ctx.exec_policy_manager.is_allowed(fallback, &cache));
        if !(allowed_primary || allowed_fallback) {
            tags.push("exec_decision:denied_exec_policy".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "run_command blocked by execution policy: {line}"
            ))
            .with_policy_tags(tags));
        }
        tags.push("exec_decision:allowed_exec_policy".to_string());
    }
    Ok(tags)
}
