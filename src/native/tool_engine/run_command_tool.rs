use super::graph_query_tool::mark_runtime_graph_dirty;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RunCommandInput {
    pub(super) command: String,
    #[serde(default)]
    pub(super) args: Vec<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RunCommandOutput {
    pub(super) exit_code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
    pub(super) timed_out: bool,
}

struct ManagedProxyRuntime {
    stop: Arc<AtomicBool>,
    handles: Vec<JoinHandle<()>>,
    http_addr: String,
    socks5_addr: Option<String>,
    admin_addr: String,
}

impl ManagedProxyRuntime {
    fn start(policy: &NativeNetworkPolicy) -> Result<Self, NativeToolEngineError> {
        let (http_bind, _http_clamped) = clamp_bind_address(
            &policy.proxy_http_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (admin_bind, _admin_clamped) = clamp_bind_address(
            &policy.proxy_admin_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );
        let (socks_bind, _socks_clamped) = clamp_bind_address(
            &policy.proxy_socks5_bind,
            policy.proxy_allow_non_loopback,
            "127.0.0.1:0",
        );

        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();

        let http_listener = TcpListener::bind(&http_bind).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to bind managed HTTP proxy listener on '{http_bind}': {error}"
            ))
        })?;
        http_listener.set_nonblocking(true).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to configure managed HTTP proxy listener nonblocking mode: {error}"
            ))
        })?;
        let http_addr = http_listener.local_addr().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read managed HTTP proxy listener address: {error}"
            ))
        })?;
        handles.push(spawn_proxy_listener(
            stop.clone(),
            http_listener,
            false,
            format!("http://{http_addr}"),
        ));

        let mut socks5_addr = None;
        if policy.proxy_socks5_enabled {
            let socks_listener = TcpListener::bind(&socks_bind).map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to bind managed SOCKS5 listener on '{socks_bind}': {error}"
                ))
            })?;
            socks_listener.set_nonblocking(true).map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to configure managed SOCKS5 listener nonblocking mode: {error}"
                ))
            })?;
            let addr = socks_listener.local_addr().map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed to read managed SOCKS5 listener address: {error}"
                ))
            })?;
            let rendered = format!("socks5://{addr}");
            socks5_addr = Some(rendered.clone());
            handles.push(spawn_proxy_listener(
                stop.clone(),
                socks_listener,
                true,
                rendered,
            ));
        }

        let admin_listener = TcpListener::bind(&admin_bind).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to bind managed proxy admin listener on '{admin_bind}': {error}"
            ))
        })?;
        admin_listener.set_nonblocking(true).map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to configure managed admin listener nonblocking mode: {error}"
            ))
        })?;
        let admin_addr = admin_listener.local_addr().map_err(|error| {
            NativeToolEngineError::execution(format!(
                "failed to read managed proxy admin address: {error}"
            ))
        })?;
        handles.push(spawn_admin_listener(
            stop.clone(),
            admin_listener,
            format!("http://{admin_addr}"),
            format!("http://{http_addr}"),
            socks5_addr.clone(),
        ));

        Ok(Self {
            stop,
            handles,
            http_addr: format!("http://{http_addr}"),
            socks5_addr,
            admin_addr: format!("http://{admin_addr}"),
        })
    }

    fn apply_proxy_env(&self, cmd: &mut Command) {
        cmd.env("HTTP_PROXY", &self.http_addr);
        cmd.env("HTTPS_PROXY", &self.http_addr);
        cmd.env("http_proxy", &self.http_addr);
        cmd.env("https_proxy", &self.http_addr);
        cmd.env("NO_PROXY", "localhost,127.0.0.1,::1");
        cmd.env("HIVEMIND_NATIVE_NETWORK_PROXY_ADMIN", &self.admin_addr);
        if let Some(socks5) = self.socks5_addr.as_ref() {
            cmd.env("ALL_PROXY", socks5);
        }
    }
}

impl Drop for ManagedProxyRuntime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_run_command(
    ctx: &ToolExecutionContext<'_>,
    input: &Value,
    default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let worktree_baseline = capture_worktree_baseline(ctx.worktree).ok();
    let mut input = decode_input::<RunCommandInput>(input)?;
    if input.args.is_empty() {
        if let Some((command, args)) = split_command_string(&input.command) {
            input.command = command;
            input.args = args;
        }
    }
    let raw_command = input.command.trim();
    if raw_command.is_empty() {
        return Err(NativeToolEngineError::validation(
            "run_command requires a non-empty command",
        ));
    }
    let command = normalize_exec_command(raw_command, ctx.env);

    let command_line = if input.args.is_empty() {
        command.clone()
    } else {
        format!("{command} {}", input.args.join(" "))
    };
    let raw_command_line = if input.args.is_empty() {
        raw_command.to_string()
    } else {
        format!("{raw_command} {}", input.args.join(" "))
    };

    if let Some(scope) = ctx.scope {
        if !scope.execution.is_allowed(&command_line)
            && (raw_command_line == command_line || !scope.execution.is_allowed(&raw_command_line))
        {
            return Err(NativeToolEngineError::scope_violation(format!(
                "run_command blocked by execution scope: {command_line}"
            )));
        }
    }
    let timeout_ms = input
        .timeout_ms
        .unwrap_or(default_timeout_ms)
        .min(default_timeout_ms);
    let network_targets = extract_network_targets(&command, &input.args);
    let managed_proxy = if matches!(
        ctx.network_policy.proxy_mode,
        NativeNetworkProxyMode::Managed
    ) {
        Some(ManagedProxyRuntime::start(&ctx.network_policy)?)
    } else {
        None
    };

    let mut cmd = Command::new(&command);
    cmd.args(&input.args)
        .current_dir(ctx.worktree)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in deterministic_env_pairs(ctx.env) {
        cmd.env(key, value);
    }
    if let Some(proxy) = managed_proxy.as_ref() {
        proxy.apply_proxy_env(&mut cmd);
    }
    let mut child = cmd.spawn().map_err(|error| {
        NativeToolEngineError::execution(format!("failed to execute '{command_line}': {error}"))
    })?;

    let started = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|error| {
                NativeToolEngineError::execution(format!(
                    "failed while waiting on '{command_line}': {error}"
                ))
            })?
            .is_some()
        {
            break;
        }
        if started.elapsed() > Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeToolEngineError::timeout("run_command", timeout_ms));
        }
        if matches!(
            ctx.network_policy.approval_mode,
            NativeNetworkApprovalMode::Deferred
        ) && !network_targets.is_empty()
        {
            if let Some(path) = ctx.network_policy.deferred_decisions_file.as_ref() {
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
                        let _ = child.kill();
                        let _ = child.wait();
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
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().map_err(|error| {
        NativeToolEngineError::execution(format!(
            "failed to collect output for '{command_line}': {error}"
        ))
    })?;

    let status = output.status.code().unwrap_or(-1);
    let result = RunCommandOutput {
        exit_code: status,
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        timed_out: false,
    };
    if status == 0 {
        match worktree_baseline.as_ref() {
            Some(previous) => match capture_worktree_dirty_paths(ctx.worktree, previous) {
                Ok((dirty_paths, _)) if !dirty_paths.is_empty() => {
                    mark_runtime_graph_dirty(ctx.env, &dirty_paths);
                }
                Ok(_) => {}
                Err(_) => mark_runtime_graph_dirty(ctx.env, &[]),
            },
            None => mark_runtime_graph_dirty(ctx.env, &[]),
        }
    }
    serde_json::to_value(result).map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode run_command output: {error}"))
    })
}
