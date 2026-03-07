use super::*;

pub(crate) fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub(crate) fn parse_bool_with_default(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => true,
        "0" | "false" | "no" | "off" => false,
        _ => default,
    }
}

pub(crate) fn current_platform_tag() -> &'static str {
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

pub(crate) fn default_sandbox_mode_for_platform() -> NativeSandboxMode {
    if current_platform_tag() == "windows" {
        NativeSandboxMode::HostPassthrough
    } else {
        NativeSandboxMode::WorkspaceWrite
    }
}

pub(crate) fn parse_sandbox_mode(raw: &str) -> Option<NativeSandboxMode> {
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

pub(crate) fn parse_approval_mode(raw: &str) -> Option<NativeApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "never" => Some(NativeApprovalMode::Never),
        "on-failure" | "on_failure" => Some(NativeApprovalMode::OnFailure),
        "on-request" | "on_request" => Some(NativeApprovalMode::OnRequest),
        "unless-trusted" | "unless_trusted" => Some(NativeApprovalMode::UnlessTrusted),
        _ => None,
    }
}

pub(crate) fn parse_network_proxy_mode(raw: &str) -> Option<NativeNetworkProxyMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" => Some(NativeNetworkProxyMode::Off),
        "managed" => Some(NativeNetworkProxyMode::Managed),
        _ => None,
    }
}

pub(crate) fn parse_network_access_mode(raw: &str) -> Option<NativeNetworkAccessMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "full" => Some(NativeNetworkAccessMode::Full),
        "limited" => Some(NativeNetworkAccessMode::Limited),
        "disabled" | "none" | "deny-all" | "deny_all" => Some(NativeNetworkAccessMode::Disabled),
        _ => None,
    }
}

pub(crate) fn parse_network_approval_mode(raw: &str) -> Option<NativeNetworkApprovalMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" | "off" => Some(NativeNetworkApprovalMode::None),
        "immediate" => Some(NativeNetworkApprovalMode::Immediate),
        "deferred" => Some(NativeNetworkApprovalMode::Deferred),
        _ => None,
    }
}
