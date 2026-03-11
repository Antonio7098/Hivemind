use super::*;

pub(super) mod chat;
mod flows;
mod governance;
mod graphs;
mod operations;
mod projects;
mod queries;
mod tasks;

const GET_ONLY_PATHS: &[&str] = &[
    "/health",
    "/api/version",
    "/api/catalog",
    "/api/state",
    "/api/chat/sessions",
    "/api/chat/sessions/inspect",
    "/api/projects",
    "/api/tasks",
    "/api/graphs",
    "/api/flows",
    "/api/merges",
    "/api/runtimes",
    "/api/runtimes/health",
    "/api/events",
    "/api/events/inspect",
    "/api/runtime-stream",
    "/api/verify/results",
    "/api/attempts/inspect",
    "/api/attempts/diff",
    "/api/flows/replay",
    "/api/worktrees",
    "/api/worktrees/inspect",
    "/api/governance/constitution",
    "/api/governance/documents",
    "/api/governance/documents/inspect",
    "/api/governance/notepad",
    "/api/governance/global/notepad",
    "/api/governance/global/skills",
    "/api/governance/global/skills/inspect",
    "/api/governance/global/templates",
    "/api/governance/global/templates/inspect",
];

const POST_ONLY_PATHS: &[&str] = &[
    "/api/chat/invoke",
    "/api/chat/sessions/create",
    "/api/chat/sessions/send",
    "/api/projects/create",
    "/api/projects/update",
    "/api/projects/delete",
    "/api/projects/runtime",
    "/api/runtime/defaults",
    "/api/projects/repos/attach",
    "/api/projects/repos/detach",
    "/api/tasks/create",
    "/api/tasks/update",
    "/api/tasks/delete",
    "/api/tasks/runtime",
    "/api/tasks/run-mode",
    "/api/tasks/close",
    "/api/tasks/start",
    "/api/tasks/complete",
    "/api/tasks/retry",
    "/api/tasks/abort",
    "/api/graphs/create",
    "/api/graphs/delete",
    "/api/graphs/dependencies/add",
    "/api/graphs/checks/add",
    "/api/graphs/validate",
    "/api/flows/create",
    "/api/flows/delete",
    "/api/flows/start",
    "/api/flows/tick",
    "/api/flows/pause",
    "/api/flows/resume",
    "/api/flows/abort",
    "/api/flows/run-mode",
    "/api/flows/dependencies/add",
    "/api/flows/runtime",
    "/api/verify/override",
    "/api/verify/run",
    "/api/merge/prepare",
    "/api/merge/approve",
    "/api/merge/execute",
    "/api/checkpoints/complete",
    "/api/worktrees/cleanup",
    "/api/worktrees/restore-turn",
    "/api/governance/constitution/check",
    "/api/governance/graph-snapshot/refresh",
];

pub(super) fn handle_api_request_inner(
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<ApiResponse> {
    if method == ApiMethod::Options {
        let mut resp = ApiResponse::text(204, "text/plain", "");
        resp.extra_headers.extend(cors_headers());
        return Ok(resp);
    }

    let (path, _qs) = url.split_once('?').unwrap_or((url, ""));

    match method {
        ApiMethod::Get => {
            if let Some(resp) = chat::handle_get(path, url, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = queries::handle_get(path, url, default_events_limit, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = governance::handle_get(path, url, registry)? {
                return Ok(resp);
            }
            if GET_ONLY_PATHS.contains(&path) {
                return method_not_allowed(path, method, "GET");
            }
            if POST_ONLY_PATHS.contains(&path) {
                return method_not_allowed(path, method, "POST");
            }
            not_found(path)
        }
        ApiMethod::Post => {
            if let Some(resp) = chat::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = projects::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = tasks::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = graphs::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = flows::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = operations::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if let Some(resp) = governance::handle_post(path, body, registry)? {
                return Ok(resp);
            }
            if GET_ONLY_PATHS.contains(&path) {
                return method_not_allowed(path, method, "GET");
            }
            if POST_ONLY_PATHS.contains(&path) {
                return method_not_allowed(path, method, "POST");
            }
            not_found(path)
        }
        ApiMethod::Options => unreachable!(),
    }
}

pub(super) fn json_ok<T: serde::Serialize>(value: T) -> Result<ApiResponse> {
    let wrapped = CliResponse::success(value);
    let mut resp = ApiResponse::json(200, &wrapped)?;
    resp.extra_headers.extend(cors_headers());
    Ok(resp)
}

fn not_found(path: &str) -> Result<ApiResponse> {
    let err = HivemindError::user(
        "endpoint_not_found",
        format!("Unknown endpoint '{path}'"),
        "server:handle_api_request",
    );
    let wrapped = CliResponse::<()>::error(&err);
    let mut resp = ApiResponse::json(404, &wrapped)?;
    resp.extra_headers.extend(cors_headers());
    Ok(resp)
}
