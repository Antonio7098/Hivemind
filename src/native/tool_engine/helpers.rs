use super::*;

pub(super) fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(super) fn parse_bool_with_default(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

pub(super) fn current_platform_tag() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

pub(super) fn default_sandbox_mode_for_platform() -> NativeSandboxMode {
    if current_platform_tag() == "windows" {
        NativeSandboxMode::HostPassthrough
    } else {
        NativeSandboxMode::WorkspaceWrite
    }
}

pub(super) fn parse_sandbox_mode(raw: &str) -> Option<NativeSandboxMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "read-only" | "read_only" => Some(NativeSandboxMode::ReadOnly),
        "workspace-write" | "workspace_write" => Some(NativeSandboxMode::WorkspaceWrite),
        "danger-full-access" | "danger_full_access" => Some(NativeSandboxMode::DangerFullAccess),
        "host-passthrough" | "host_passthrough" | "host" => {
            Some(NativeSandboxMode::HostPassthrough)
        }
        _ => None,
    }
}

pub(super) fn parse_approval_mode(raw: &str) -> Option<NativeApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "never" => Some(NativeApprovalMode::Never),
        "on-failure" | "on_failure" => Some(NativeApprovalMode::OnFailure),
        "on-request" | "on_request" => Some(NativeApprovalMode::OnRequest),
        "unless-trusted" | "unless_trusted" => Some(NativeApprovalMode::UnlessTrusted),
        _ => None,
    }
}

pub(super) fn parse_network_proxy_mode(raw: &str) -> Option<NativeNetworkProxyMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => Some(NativeNetworkProxyMode::Off),
        "managed" => Some(NativeNetworkProxyMode::Managed),
        _ => None,
    }
}

pub(super) fn parse_network_access_mode(raw: &str) -> Option<NativeNetworkAccessMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "full" => Some(NativeNetworkAccessMode::Full),
        "limited" => Some(NativeNetworkAccessMode::Limited),
        "disabled" | "none" | "deny-all" | "deny_all" => Some(NativeNetworkAccessMode::Disabled),
        _ => None,
    }
}

pub(super) fn parse_network_approval_mode(raw: &str) -> Option<NativeNetworkApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => Some(NativeNetworkApprovalMode::None),
        "immediate" => Some(NativeNetworkApprovalMode::Immediate),
        "deferred" => Some(NativeNetworkApprovalMode::Deferred),
        _ => None,
    }
}

pub(super) fn spawn_proxy_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    is_socks5: bool,
    endpoint: String,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if is_socks5 {
                        let _ = stream.write_all(&[0x05, 0xFF]);
                    } else {
                        let _ = respond_proxy_denied(&mut stream, &endpoint);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

pub(super) fn spawn_admin_listener(
    stop: Arc<AtomicBool>,
    listener: TcpListener,
    endpoint: String,
    http_proxy: String,
    socks_proxy: Option<String>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let body = serde_json::to_string(&json!({
                        "ok": true,
                        "admin_endpoint": endpoint,
                        "http_proxy": http_proxy,
                        "socks5_proxy": socks_proxy,
                    }))
                    .unwrap_or_else(|_| "{\"ok\":false}".to_string());
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => break,
            }
        }
    })
}

pub(super) fn respond_proxy_denied(stream: &mut TcpStream, endpoint: &str) -> std::io::Result<()> {
    let body = format!(
        "{{\"ok\":false,\"code\":\"network_proxy_denied\",\"message\":\"managed network proxy denied request\",\"admin\":\"{endpoint}\"}}"
    );
    let response = format!(
        "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

pub(super) fn clamp_bind_address(
    raw: &str,
    allow_non_loopback: bool,
    fallback: &str,
) -> (String, bool) {
    let candidate = if raw.trim().is_empty() {
        fallback.to_string()
    } else {
        raw.trim().to_string()
    };
    let mut parts = candidate.rsplitn(2, ':');
    let port_raw = parts.next().unwrap_or("0");
    let host_raw = parts.next().unwrap_or("127.0.0.1");
    let port = port_raw.parse::<u16>().unwrap_or(0);
    let mut host = host_raw.to_string();
    let clamped = if !allow_non_loopback && !is_loopback_host(&host) {
        host = "127.0.0.1".to_string();
        true
    } else {
        false
    };
    (format!("{host}:{port}"), clamped)
}

pub(super) fn is_loopback_host(host: &str) -> bool {
    let normalized = host.trim().trim_matches('[').trim_matches(']');
    if normalized.eq_ignore_ascii_case("localhost") {
        return true;
    }
    normalized
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

pub(super) fn normalize_exec_command(command: &str, env_map: &HashMap<String, String>) -> String {
    let normalized = command.trim();
    if !normalized.eq_ignore_ascii_case("python") {
        return normalized.to_string();
    }
    if command_exists_in_path(normalized, env_map) {
        return normalized.to_string();
    }
    if command_exists_in_path("python3", env_map) {
        return "python3".to_string();
    }
    normalized.to_string()
}

pub(super) fn command_exists_in_path(command: &str, env_map: &HashMap<String, String>) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_path_components = Path::new(trimmed).components().count() > 1;
    if has_path_components || Path::new(trimmed).is_absolute() {
        return resolve_command_path(trimmed, env_map).is_some();
    }

    let path_value = env_lookup(env_map, "PATH")
        .map(str::to_string)
        .or_else(|| env::var("PATH").ok())
        .unwrap_or_default();
    if path_value.trim().is_empty() {
        return false;
    }
    let windows_exts = windows_command_extensions(env_map);

    env::split_paths(&path_value).any(|dir| {
        let candidate = dir.join(trimmed);
        is_executable_path(&candidate)
            || windows_exts
                .iter()
                .any(|ext| is_executable_path(&dir.join(format!("{trimmed}.{ext}"))))
    })
}

pub(super) fn resolve_command_path(
    command: &str,
    env_map: &HashMap<String, String>,
) -> Option<PathBuf> {
    let candidate = PathBuf::from(command);
    if is_executable_path(&candidate) {
        return Some(candidate);
    }

    for ext in windows_command_extensions(env_map) {
        let ext = ext.trim_start_matches('.');
        let fallback = PathBuf::from(format!("{command}.{ext}"));
        if is_executable_path(&fallback) {
            return Some(fallback);
        }
    }

    None
}

pub(super) fn env_lookup<'a>(env_map: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env_map.get(key).map(String::as_str).or_else(|| {
        env_map
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    })
}

pub(super) fn windows_command_extensions(env_map: &HashMap<String, String>) -> Vec<String> {
    #[cfg(windows)]
    {
        let raw = env_lookup(env_map, "PATHEXT")
            .map(str::to_string)
            .or_else(|| env::var("PATHEXT").ok())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
        return raw
            .split(';')
            .filter_map(|entry| {
                let trimmed = entry.trim().trim_start_matches('.').to_ascii_lowercase();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .collect();
    }

    #[cfg(not(windows))]
    {
        let _ = env_map;
        Vec::new()
    }
}

pub(super) fn is_executable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .ok()
            .is_some_and(|meta| (meta.permissions().mode() & 0o111) != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(super) fn extract_network_targets(command: &str, args: &[String]) -> Vec<NativeNetworkTarget> {
    let method = infer_network_method(command, args);
    let mut targets = BTreeMap::<String, NativeNetworkTarget>::new();
    for arg in args {
        if let Some(url) = arg.strip_prefix("--url=") {
            add_network_target_from_url(url, &method, &mut targets);
            continue;
        }
        if let Some(url) = arg.strip_prefix("url=") {
            add_network_target_from_url(url, &method, &mut targets);
            continue;
        }
        add_network_target_from_url(arg, &method, &mut targets);
    }
    targets.into_values().collect()
}

pub(super) fn add_network_target_from_url(
    candidate: &str,
    method: &str,
    targets: &mut BTreeMap<String, NativeNetworkTarget>,
) {
    let Ok(parsed) = reqwest::Url::parse(candidate) else {
        return;
    };
    let Some(host) = parsed.host_str() else {
        return;
    };
    let protocol = parsed.scheme().to_ascii_lowercase();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let target = NativeNetworkTarget {
        protocol,
        host: host.to_ascii_lowercase(),
        port,
        method: method.to_ascii_uppercase(),
    };
    targets.insert(target.cache_key(), target);
}

pub(super) fn infer_network_method(command: &str, args: &[String]) -> String {
    let lowered = command.trim().to_ascii_lowercase();
    if lowered == "curl" {
        if args.iter().any(|arg| arg.eq_ignore_ascii_case("-I")) {
            return "HEAD".to_string();
        }
        for window in args.windows(2) {
            if window.first().is_some_and(|flag| {
                flag.eq_ignore_ascii_case("-X") || flag.eq_ignore_ascii_case("--request")
            }) {
                return window[1].to_ascii_uppercase();
            }
        }
        return "GET".to_string();
    }
    if lowered == "wget" {
        return "GET".to_string();
    }
    if lowered == "git" {
        return "FETCH".to_string();
    }
    "CONNECT".to_string()
}

pub(super) fn matches_host_pattern(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim().to_ascii_lowercase();
    let host = host.trim().to_ascii_lowercase();
    if pattern.is_empty() {
        return false;
    }
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    if let Some(suffix) = pattern.strip_prefix('.') {
        return host.ends_with(&format!(".{suffix}"));
    }
    host == pattern
}

pub(super) fn is_private_or_local_host(host: &str) -> bool {
    let normalized = host.trim().to_ascii_lowercase();
    if normalized == "localhost" {
        return true;
    }
    if normalized
        .rsplit('.')
        .next()
        .is_some_and(|suffix| suffix == "local" || suffix == "internal")
    {
        return true;
    }
    let Ok(ip) = normalized.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4 == Ipv4Addr::UNSPECIFIED
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_unspecified()
        }
    }
}

pub(super) fn format_network_target_tag_value(target: &NativeNetworkTarget) -> String {
    sanitize_policy_tag_value(&format!(
        "{}://{}:{}@{}",
        target.protocol, target.host, target.port, target.method
    ))
}

pub(super) fn read_deferred_network_decisions(path: &str) -> Vec<NativeDeferredNetworkDecision> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
                let target = value.get("target")?.as_str()?.to_string();
                let decision = value.get("decision")?.as_str()?.to_ascii_lowercase();
                return Some(NativeDeferredNetworkDecision {
                    target_key: target,
                    deny: decision == "deny",
                });
            }
            let mut parts = trimmed.splitn(2, ',');
            let target = parts.next()?.trim().to_string();
            let decision = parts.next()?.trim().to_ascii_lowercase();
            Some(NativeDeferredNetworkDecision {
                target_key: target,
                deny: decision == "deny",
            })
        })
        .collect()
}

pub(super) fn sanitize_policy_tag_value(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

pub(super) fn is_broad_prefix(prefix: &str) -> bool {
    let normalized = prefix.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized == "*"
        || normalized == "."
        || normalized == "/"
        || normalized == "\\"
        || normalized == "~"
        || normalized == "c:"
        || normalized == "c:\\"
    {
        return true;
    }
    if normalized.contains('*') {
        return true;
    }
    matches!(
        normalized.as_str(),
        "sh" | "bash" | "zsh" | "fish" | "cmd" | "cmd.exe" | "powershell" | "pwsh"
    )
}
