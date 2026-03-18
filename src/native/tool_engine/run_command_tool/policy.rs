use super::*;

pub(super) fn enforce_execution_scope(
    ctx: &ToolExecutionContext<'_>,
    command_line: &str,
    raw_command_line: &str,
) -> Result<(), NativeToolEngineError> {
    if let Some(scope) = ctx.scope {
        if !scope.execution.is_allowed(command_line)
            && (raw_command_line == command_line || !scope.execution.is_allowed(raw_command_line))
        {
            return Err(NativeToolEngineError::scope_violation(format!(
                "run_command blocked by execution scope: {command_line}"
            )));
        }
    }
    Ok(())
}

pub(super) fn start_managed_proxy_if_needed(
    ctx: &ToolExecutionContext<'_>,
) -> Result<Option<ManagedProxyRuntime>, NativeToolEngineError> {
    if matches!(
        ctx.network_policy.proxy_mode,
        NativeNetworkProxyMode::Managed
    ) {
        Ok(Some(ManagedProxyRuntime::start(&ctx.network_policy)?))
    } else {
        Ok(None)
    }
}

pub(super) fn apply_deferred_network_decisions(
    ctx: &ToolExecutionContext<'_>,
    network_targets: &[NativeNetworkTarget],
    child: &mut Child,
) -> Result<(), NativeToolEngineError> {
    if !matches!(
        ctx.network_policy.approval_mode,
        NativeNetworkApprovalMode::Deferred
    ) || network_targets.is_empty()
    {
        return Ok(());
    }

    let Some(path) = ctx.network_policy.deferred_decisions_file.as_ref() else {
        return Ok(());
    };

    let decisions = read_deferred_network_decisions(path);
    let keys = network_targets
        .iter()
        .map(NativeNetworkTarget::cache_key)
        .collect::<Vec<_>>();

    for decision in decisions {
        if !keys.iter().any(|key| key == &decision.target_key) {
            continue;
        }
        if decision.deny {
            terminate_child_process_group(child);
            let mut tags = ctx.network_policy.base_policy_tags();
            tags.push("network_approval_outcome:deferred_denied".to_string());
            tags.push(format!(
                "network_target:{}",
                sanitize_policy_tag_value(&decision.target_key)
            ));
            return Err(NativeToolEngineError::policy_violation(format!(
                "network deferred denial received for '{}'",
                decision.target_key
            ))
            .with_policy_tags(tags));
        }
        let mut cache = ctx.network_approval_cache.borrow_mut();
        cache.insert_bounded(
            decision.target_key,
            ctx.network_policy.approval_cache_max_entries,
        );
    }

    Ok(())
}
