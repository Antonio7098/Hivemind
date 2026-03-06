use super::*;

pub(super) fn parse_query(url: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some((_path, qs)) = url.split_once('?') else {
        return out;
    };

    for part in qs.split('&') {
        if part.trim().is_empty() {
            continue;
        }

        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        out.insert(k.to_string(), v.to_string());
    }

    out
}

pub(super) fn cors_headers() -> Vec<tiny_http::Header> {
    vec![
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
            .expect("static header"),
        tiny_http::Header::from_bytes(
            &b"Access-Control-Allow-Methods"[..],
            &b"GET, POST, OPTIONS"[..],
        )
        .expect("static header"),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..])
            .expect("static header"),
    ]
}

pub(super) fn parse_json_body<T: for<'de> Deserialize<'de>>(
    body: Option<&[u8]>,
    origin: &str,
) -> Result<T> {
    let raw = body.ok_or_else(|| {
        HivemindError::user("request_body_required", "Request body is required", origin)
    })?;

    serde_json::from_slice(raw).map_err(|e| {
        HivemindError::user(
            "invalid_json_body",
            format!("Invalid JSON body: {e}"),
            origin,
        )
    })
}

pub(super) fn parse_runtime_role(raw: Option<&str>, origin: &str) -> Result<RuntimeRole> {
    match raw.unwrap_or("worker").to_lowercase().as_str() {
        "worker" => Ok(RuntimeRole::Worker),
        "validator" => Ok(RuntimeRole::Validator),
        other => Err(HivemindError::user(
            "invalid_runtime_role",
            format!("Invalid runtime role '{other}'"),
            origin,
        )
        .with_hint("Use 'worker' or 'validator'")),
    }
}

pub(super) fn parse_run_mode(raw: &str, origin: &str) -> Result<RunMode> {
    match raw.to_lowercase().as_str() {
        "auto" => Ok(RunMode::Auto),
        "manual" => Ok(RunMode::Manual),
        other => Err(HivemindError::user(
            "invalid_run_mode",
            format!("Invalid run mode '{other}'"),
            origin,
        )
        .with_hint("Use 'auto' or 'manual'")),
    }
}

pub(super) fn parse_merge_mode(raw: Option<&str>, origin: &str) -> Result<MergeExecuteMode> {
    match raw.unwrap_or("local").to_lowercase().as_str() {
        "local" => Ok(MergeExecuteMode::Local),
        "pr" => Ok(MergeExecuteMode::Pr),
        other => Err(HivemindError::user(
            "invalid_merge_mode",
            format!("Invalid merge mode '{other}'"),
            origin,
        )
        .with_hint("Use 'local' or 'pr'")),
    }
}

pub(super) fn env_pairs_from_map(env: Option<HashMap<String, String>>) -> Vec<String> {
    let mut env_pairs = Vec::new();
    if let Some(env) = env {
        for (k, v) in env {
            env_pairs.push(format!("{k}={v}"));
        }
    }
    env_pairs
}

pub(super) fn method_not_allowed(
    path: &str,
    method: ApiMethod,
    allowed: &'static str,
) -> Result<ApiResponse> {
    let err = HivemindError::user(
        "method_not_allowed",
        format!("Method '{method:?}' is not allowed for '{path}'"),
        "server:handle_api_request",
    )
    .with_hint(format!("Allowed method(s): {allowed}"));
    let wrapped = CliResponse::<()>::error(&err);
    let mut resp = ApiResponse::json(405, &wrapped)?;
    resp.extra_headers.extend(cors_headers());
    Ok(resp)
}
