use super::*;

pub(super) fn evaluate_network_targets(
    ctx: &ToolExecutionContext<'_>,
    network_targets: &[NativeNetworkTarget],
    tags: &mut Vec<String>,
) -> Result<(), NativeToolEngineError> {
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
        for target in network_targets {
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
                .with_policy_tags(tags.clone()));
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
                .with_policy_tags(tags.clone()));
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
                .with_policy_tags(tags.clone()));
            }
            if ctx.network_policy.block_private_addresses
                && is_private_or_local_host(&target.host)
            {
                tags.push("network_decision:denied_private_address".to_string());
                return Err(NativeToolEngineError::policy_violation(format!(
                    "network policy denied '{}': private/local address blocked",
                    target.cache_key()
                ))
                .with_policy_tags(tags.clone()));
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
                .with_policy_tags(tags.clone()));
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
                        .with_policy_tags(tags.clone()));
                    }
                }
                NativeNetworkApprovalMode::Deferred => {
                    tags.push("network_approval_required:true".to_string());
                    if ctx.network_policy.deferred_decisions_file.is_none() {
                        tags.push("network_approval_outcome:denied_no_watcher".to_string());
                        return Err(NativeToolEngineError::policy_violation(format!(
                            "network approval deferred mode requires {NETWORK_APPROVAL_DEFERRED_DECISIONS_FILE_ENV_KEY}"
                        ))
                        .with_policy_tags(tags.clone()));
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
    Ok(())
}
