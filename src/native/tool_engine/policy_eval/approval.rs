use super::*;

pub(super) fn evaluate_approval_and_exec_policy(
    ctx: &ToolExecutionContext<'_>,
    requires_write: bool,
    requires_exec: bool,
    approval_cache_key: &str,
    command_line: Option<&str>,
    fallback_command_line: Option<&str>,
    dangerous_reason: Option<String>,
    tags: &mut Vec<String>,
) -> Result<(), NativeToolEngineError> {
    let trusted = command_line.is_some_and(|line| {
        ctx.approval_policy
            .trusted_prefixes
            .iter()
            .any(|prefix| matches_command_pattern(prefix, line))
    }) || fallback_command_line.is_some_and(|line| {
        ctx.approval_policy
            .trusted_prefixes
            .iter()
            .any(|prefix| matches_command_pattern(prefix, line))
    });

    let mut approval_required = false;
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
        if is_broad_prefix(approval_cache_key) {
            tags.push("approval_outcome:denied_broad_prefix".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "approval denied for broad prefix '{approval_cache_key}'; provide a specific command prefix"
            ))
            .with_policy_tags(tags.clone()));
        }
        let mut cache = ctx.approval_cache.borrow_mut();
        if cache.contains(approval_cache_key) {
            tags.push("approval_review_decision:cached".to_string());
            tags.push("approval_outcome:approved_for_session".to_string());
            tags.push("approved_for_session:true".to_string());
        } else if ctx.approval_policy.review_decision == NativeApprovalReviewDecision::Approve {
            cache.insert_bounded(
                approval_cache_key.to_string(),
                ctx.approval_policy.cache_max_entries,
            );
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
            .with_policy_tags(tags.clone()));
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
            return Err(NativeToolEngineError::policy_violation(format!(
                "dangerous command denied: {reason}. Set {SANDBOX_MODE_ENV_KEY}=danger-full-access (or host-passthrough) and request approval."
            ))
            .with_policy_tags(tags.clone()));
        }
        if matches!(
            ctx.approval_policy.mode,
            NativeApprovalMode::Never | NativeApprovalMode::OnFailure
        ) {
            tags.push("exec_decision:denied_dangerous_requires_preapproval".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "dangerous command denied: {reason}. Set {APPROVAL_MODE_ENV_KEY} to on-request or unless-trusted."
            ))
            .with_policy_tags(tags.clone()));
        }
    }

    if let Some(line) = command_line {
        let cache = ctx.approval_cache.borrow();
        let allowed_primary = ctx.exec_policy_manager.is_allowed(line, &cache);
        let allowed_fallback = fallback_command_line
            .is_some_and(|fallback| ctx.exec_policy_manager.is_allowed(fallback, &cache));
        if !(allowed_primary || allowed_fallback) {
            tags.push("exec_decision:denied_exec_policy".to_string());
            return Err(NativeToolEngineError::policy_violation(format!(
                "run_command blocked by execution policy: {line}"
            ))
            .with_policy_tags(tags.clone()));
        }
        tags.push("exec_decision:allowed_exec_policy".to_string());
    }

    Ok(())
}
