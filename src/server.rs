use crate::cli::output::CliResponse;
use crate::core::error::{HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload};
use crate::core::registry::Registry;
use crate::core::state::{MergeState, Project, Task};
use crate::core::{flow::TaskFlow, graph::TaskGraph};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
    pub events_limit: usize,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8787,
            events_limit: 200,
        }
    }
}

pub fn handle_api_request(
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
) -> Result<ApiResponse> {
    let registry = Registry::open()?;
    handle_api_request_inner(method, url, default_events_limit, &registry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    Options,
}

impl ApiMethod {
    fn from_http(method: &tiny_http::Method) -> Option<Self> {
        match method {
            tiny_http::Method::Get => Some(Self::Get),
            tiny_http::Method::Options => Some(Self::Options),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status_code: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    pub extra_headers: Vec<tiny_http::Header>,
}

impl ApiResponse {
    fn json<T: Serialize>(status_code: u16, value: &T) -> Result<Self> {
        let body = serde_json::to_vec_pretty(value).map_err(|e| {
            HivemindError::system("json_serialize_failed", e.to_string(), "server:json")
        })?;
        Ok(Self {
            status_code,
            content_type: "application/json",
            body,
            extra_headers: Vec::new(),
        })
    }

    fn text(status_code: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status_code,
            content_type,
            body: body.into(),
            extra_headers: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UiState {
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub graphs: Vec<TaskGraph>,
    pub flows: Vec<TaskFlow>,
    pub merge_states: Vec<MergeState>,
    pub events: Vec<UiEvent>,
}

#[derive(Debug, Serialize)]
pub struct UiEvent {
    pub id: String,
    pub r#type: String,
    pub category: String,
    pub timestamp: DateTime<Utc>,
    pub sequence: Option<u64>,
    pub correlation: CorrelationIds,
    pub payload: HashMap<String, Value>,
}

fn payload_pascal_type(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ErrorOccurred { .. } => "ErrorOccurred",
        EventPayload::ProjectCreated { .. } => "ProjectCreated",
        EventPayload::ProjectUpdated { .. } => "ProjectUpdated",
        EventPayload::ProjectRuntimeConfigured { .. } => "ProjectRuntimeConfigured",
        EventPayload::RepositoryAttached { .. } => "RepositoryAttached",
        EventPayload::RepositoryDetached { .. } => "RepositoryDetached",
        EventPayload::TaskCreated { .. } => "TaskCreated",
        EventPayload::TaskUpdated { .. } => "TaskUpdated",
        EventPayload::TaskClosed { .. } => "TaskClosed",
        EventPayload::TaskGraphCreated { .. } => "TaskGraphCreated",
        EventPayload::TaskAddedToGraph { .. } => "TaskAddedToGraph",
        EventPayload::DependencyAdded { .. } => "DependencyAdded",
        EventPayload::ScopeAssigned { .. } => "ScopeAssigned",
        EventPayload::TaskFlowCreated { .. } => "TaskFlowCreated",
        EventPayload::TaskFlowStarted { .. } => "TaskFlowStarted",
        EventPayload::TaskFlowPaused { .. } => "TaskFlowPaused",
        EventPayload::TaskFlowResumed { .. } => "TaskFlowResumed",
        EventPayload::TaskFlowCompleted { .. } => "TaskFlowCompleted",
        EventPayload::TaskFlowAborted { .. } => "TaskFlowAborted",
        EventPayload::TaskReady { .. } => "TaskReady",
        EventPayload::TaskBlocked { .. } => "TaskBlocked",
        EventPayload::TaskExecutionStateChanged { .. } => "TaskExecutionStateChanged",
        EventPayload::AttemptStarted { .. } => "AttemptStarted",
        EventPayload::BaselineCaptured { .. } => "BaselineCaptured",
        EventPayload::FileModified { .. } => "FileModified",
        EventPayload::DiffComputed { .. } => "DiffComputed",
        EventPayload::CheckpointCommitCreated { .. } => "CheckpointCommitCreated",
        EventPayload::ScopeValidated { .. } => "ScopeValidated",
        EventPayload::ScopeViolationDetected { .. } => "ScopeViolationDetected",
        EventPayload::TaskRetryRequested { .. } => "TaskRetryRequested",
        EventPayload::TaskAborted { .. } => "TaskAborted",
        EventPayload::HumanOverride { .. } => "HumanOverride",
        EventPayload::MergePrepared { .. } => "MergePrepared",
        EventPayload::MergeApproved { .. } => "MergeApproved",
        EventPayload::MergeCompleted { .. } => "MergeCompleted",
        EventPayload::RuntimeStarted { .. } => "RuntimeStarted",
        EventPayload::RuntimeOutputChunk { .. } => "RuntimeOutputChunk",
        EventPayload::RuntimeInputProvided { .. } => "RuntimeInputProvided",
        EventPayload::RuntimeInterrupted { .. } => "RuntimeInterrupted",
        EventPayload::RuntimeExited { .. } => "RuntimeExited",
        EventPayload::RuntimeTerminated { .. } => "RuntimeTerminated",
        EventPayload::RuntimeFilesystemObserved { .. } => "RuntimeFilesystemObserved",
        EventPayload::Unknown => "Unknown",
    }
}

fn payload_category(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ErrorOccurred { .. } => "error",

        EventPayload::ProjectCreated { .. }
        | EventPayload::ProjectUpdated { .. }
        | EventPayload::ProjectRuntimeConfigured { .. }
        | EventPayload::RepositoryAttached { .. }
        | EventPayload::RepositoryDetached { .. } => "project",

        EventPayload::TaskCreated { .. }
        | EventPayload::TaskUpdated { .. }
        | EventPayload::TaskClosed { .. } => "task",

        EventPayload::TaskGraphCreated { .. }
        | EventPayload::TaskAddedToGraph { .. }
        | EventPayload::DependencyAdded { .. }
        | EventPayload::ScopeAssigned { .. } => "graph",

        EventPayload::TaskFlowCreated { .. }
        | EventPayload::TaskFlowStarted { .. }
        | EventPayload::TaskFlowPaused { .. }
        | EventPayload::TaskFlowResumed { .. }
        | EventPayload::TaskFlowCompleted { .. }
        | EventPayload::TaskFlowAborted { .. } => "flow",

        EventPayload::TaskReady { .. }
        | EventPayload::TaskBlocked { .. }
        | EventPayload::TaskExecutionStateChanged { .. }
        | EventPayload::AttemptStarted { .. }
        | EventPayload::TaskRetryRequested { .. }
        | EventPayload::TaskAborted { .. } => "execution",

        EventPayload::HumanOverride { .. } => "verification",

        EventPayload::ScopeValidated { .. } | EventPayload::ScopeViolationDetected { .. } => {
            "scope"
        }

        EventPayload::MergePrepared { .. }
        | EventPayload::MergeApproved { .. }
        | EventPayload::MergeCompleted { .. } => "merge",

        EventPayload::RuntimeStarted { .. }
        | EventPayload::RuntimeOutputChunk { .. }
        | EventPayload::RuntimeInputProvided { .. }
        | EventPayload::RuntimeInterrupted { .. }
        | EventPayload::RuntimeExited { .. }
        | EventPayload::RuntimeTerminated { .. }
        | EventPayload::RuntimeFilesystemObserved { .. }
        | EventPayload::Unknown => "runtime",

        EventPayload::FileModified { .. }
        | EventPayload::DiffComputed { .. }
        | EventPayload::CheckpointCommitCreated { .. }
        | EventPayload::BaselineCaptured { .. } => "filesystem",
    }
}

fn payload_map(payload: &EventPayload) -> Result<HashMap<String, Value>> {
    let mut v = serde_json::to_value(payload).map_err(|e| {
        HivemindError::system(
            "payload_serialize_failed",
            e.to_string(),
            "server:payload_map",
        )
    })?;

    match &mut v {
        Value::Object(map) => {
            map.remove("type");
            Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        _ => Ok(HashMap::new()),
    }
}

fn ui_event(event: &Event) -> Result<UiEvent> {
    Ok(UiEvent {
        id: event.metadata.id.to_string(),
        r#type: payload_pascal_type(&event.payload).to_string(),
        category: payload_category(&event.payload).to_string(),
        timestamp: event.metadata.timestamp,
        sequence: event.metadata.sequence,
        correlation: event.metadata.correlation.clone(),
        payload: payload_map(&event.payload)?,
    })
}

fn build_ui_state(registry: &Registry, events_limit: usize) -> Result<UiState> {
    let state = registry.state()?;

    let mut projects: Vec<Project> = state.projects.into_values().collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));

    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();

    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();

    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();

    let mut merge_states: Vec<MergeState> = state.merge_states.into_values().collect();
    merge_states.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merge_states.reverse();

    let events = registry.list_events(None, events_limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();

    Ok(UiState {
        projects,
        tasks,
        graphs,
        flows,
        merge_states,
        events: ui_events,
    })
}

fn parse_query(url: &str) -> HashMap<String, String> {
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

fn cors_headers() -> Vec<tiny_http::Header> {
    vec![
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
            .expect("static header"),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, OPTIONS"[..])
            .expect("static header"),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..])
            .expect("static header"),
    ]
}

fn list_tasks(registry: &Registry) -> Result<Vec<Task>> {
    let state = registry.state()?;
    let mut tasks: Vec<Task> = state.tasks.into_values().collect();
    tasks.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    tasks.reverse();
    Ok(tasks)
}

fn list_graphs(registry: &Registry) -> Result<Vec<TaskGraph>> {
    let state = registry.state()?;
    let mut graphs: Vec<TaskGraph> = state.graphs.into_values().collect();
    graphs.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    graphs.reverse();
    Ok(graphs)
}

fn list_flows(registry: &Registry) -> Result<Vec<TaskFlow>> {
    let state = registry.state()?;
    let mut flows: Vec<TaskFlow> = state.flows.into_values().collect();
    flows.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    flows.reverse();
    Ok(flows)
}

fn list_merge_states(registry: &Registry) -> Result<Vec<MergeState>> {
    let state = registry.state()?;
    let mut merges: Vec<MergeState> = state.merge_states.into_values().collect();
    merges.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    merges.reverse();
    Ok(merges)
}

fn list_ui_events(registry: &Registry, limit: usize) -> Result<Vec<UiEvent>> {
    let events = registry.list_events(None, limit)?;
    let mut ui_events: Vec<UiEvent> = events.iter().map(ui_event).collect::<Result<_>>()?;
    ui_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    ui_events.reverse();
    Ok(ui_events)
}

fn handle_api_request_inner(
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
    registry: &Registry,
) -> Result<ApiResponse> {
    if method == ApiMethod::Options {
        let mut resp = ApiResponse::text(204, "text/plain", "");
        resp.extra_headers.extend(cors_headers());
        return Ok(resp);
    }

    let (path, _qs) = url.split_once('?').unwrap_or((url, ""));

    match path {
        "/health" => {
            let mut resp = ApiResponse::text(200, "text/plain", "ok\n");
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/version" => {
            let value = serde_json::json!({"version": env!("CARGO_PKG_VERSION")});
            let wrapped = CliResponse::success(value);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/state" => {
            let query = parse_query(url);
            let events_limit = query
                .get("events_limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);

            let state = build_ui_state(registry, events_limit)?;
            let wrapped = CliResponse::success(state);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects" => {
            let wrapped = CliResponse::success(registry.list_projects()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks" => {
            let wrapped = CliResponse::success(list_tasks(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs" => {
            let wrapped = CliResponse::success(list_graphs(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows" => {
            let wrapped = CliResponse::success(list_flows(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merges" => {
            let wrapped = CliResponse::success(list_merge_states(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/events" => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            let wrapped = CliResponse::success(list_ui_events(registry, limit)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        _ => {
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
    }
}

pub fn serve(config: &ServeConfig) -> Result<()> {
    let addr = format!("{}:{}", config.host, config.port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| HivemindError::system("server_bind_failed", e.to_string(), "server:serve"))?;

    eprintln!("hivemind serve listening on http://{addr}");

    for req in server.incoming_requests() {
        let Some(method) = ApiMethod::from_http(req.method()) else {
            let _ = req.respond(tiny_http::Response::empty(405));
            continue;
        };

        let response = match handle_api_request(method, req.url(), config.events_limit) {
            Ok(r) => r,
            Err(e) => {
                let wrapped = CliResponse::<()>::error(&e);
                match ApiResponse::json(500, &wrapped) {
                    Ok(mut r) => {
                        r.extra_headers.extend(cors_headers());
                        r
                    }
                    Err(_) => ApiResponse::text(500, "text/plain", "internal error\n"),
                }
            }
        };

        let mut tiny = tiny_http::Response::from_data(response.body)
            .with_status_code(response.status_code)
            .with_header(
                tiny_http::Header::from_bytes(
                    &b"Content-Type"[..],
                    response.content_type.as_bytes(),
                )
                .expect("content-type header"),
            );

        for h in response.extra_headers {
            tiny = tiny.with_header(h);
        }

        let _ = req.respond(tiny);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::{Registry, RegistryConfig};

    fn test_registry() -> Registry {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = RegistryConfig::with_dir(tmp.path().to_path_buf());
        Registry::open_with_config(config).expect("registry")
    }

    fn json_value(body: &[u8]) -> Value {
        serde_json::from_slice(body).expect("json")
    }

    #[test]
    fn api_version_ok() {
        let reg = test_registry();
        let resp = handle_api_request_inner(ApiMethod::Get, "/api/version", 10, &reg).unwrap();
        assert_eq!(resp.status_code, 200);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], true);
        assert!(v["data"]["version"].is_string());
    }

    #[test]
    fn api_state_ok_empty() {
        let reg = test_registry();
        let resp = handle_api_request_inner(ApiMethod::Get, "/api/state", 10, &reg).unwrap();
        assert_eq!(resp.status_code, 200);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], true);
        assert!(v["data"]["projects"].is_array());
    }

    #[test]
    fn api_unknown_endpoint_404() {
        let reg = test_registry();
        let resp = handle_api_request_inner(ApiMethod::Get, "/api/nope", 10, &reg).unwrap();
        assert_eq!(resp.status_code, 404);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], false);
        assert_eq!(v["error"]["code"], "endpoint_not_found");
    }
}
