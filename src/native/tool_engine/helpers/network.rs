use super::*;

pub(crate) fn extract_network_targets(command: &str, args: &[String]) -> Vec<NativeNetworkTarget> {
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

pub(crate) fn add_network_target_from_url(
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

pub(crate) fn infer_network_method(command: &str, args: &[String]) -> String {
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

pub(crate) fn matches_host_pattern(pattern: &str, host: &str) -> bool {
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

pub(crate) fn is_private_or_local_host(host: &str) -> bool {
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

pub(crate) fn format_network_target_tag_value(target: &NativeNetworkTarget) -> String {
    sanitize_policy_tag_value(&format!(
        "{}://{}:{}@{}",
        target.protocol, target.host, target.port, target.method
    ))
}

pub(crate) fn read_deferred_network_decisions(path: &str) -> Vec<NativeDeferredNetworkDecision> {
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

pub(crate) fn sanitize_policy_tag_value(raw: &str) -> String {
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

pub(crate) fn is_broad_prefix(prefix: &str) -> bool {
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
