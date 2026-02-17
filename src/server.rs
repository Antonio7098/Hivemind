use crate::cli::output::CliResponse;
use crate::core::error::{HivemindError, Result};
use crate::core::events::{CorrelationIds, Event, EventPayload, RuntimeRole};
use crate::core::flow::{RetryMode, RunMode};
use crate::core::registry::{MergeExecuteMode, MergeExecuteOptions, Registry};
use crate::core::state::{MergeState, Project, Task};
use crate::core::verification::CheckConfig;
use crate::core::{flow::TaskFlow, graph::TaskGraph};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    body: Option<&[u8]>,
) -> Result<ApiResponse> {
    let registry = Registry::open()?;
    handle_api_request_inner(method, url, default_events_limit, body, &registry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiMethod {
    Get,
    Post,
    Options,
}

impl ApiMethod {
    fn from_http(method: &tiny_http::Method) -> Option<Self> {
        match method {
            tiny_http::Method::Get => Some(Self::Get),
            tiny_http::Method::Post => Some(Self::Post),
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
        EventPayload::ProjectRuntimeRoleConfigured { .. } => "ProjectRuntimeRoleConfigured",
        EventPayload::GlobalRuntimeConfigured { .. } => "GlobalRuntimeConfigured",
        EventPayload::RepositoryAttached { .. } => "RepositoryAttached",
        EventPayload::RepositoryDetached { .. } => "RepositoryDetached",
        EventPayload::GovernanceProjectStorageInitialized { .. } => {
            "GovernanceProjectStorageInitialized"
        }
        EventPayload::GovernanceArtifactUpserted { .. } => "GovernanceArtifactUpserted",
        EventPayload::GovernanceArtifactDeleted { .. } => "GovernanceArtifactDeleted",
        EventPayload::GovernanceAttachmentLifecycleUpdated { .. } => {
            "GovernanceAttachmentLifecycleUpdated"
        }
        EventPayload::GovernanceStorageMigrated { .. } => "GovernanceStorageMigrated",
        EventPayload::ConstitutionInitialized { .. } => "ConstitutionInitialized",
        EventPayload::ConstitutionUpdated { .. } => "ConstitutionUpdated",
        EventPayload::ConstitutionValidated { .. } => "ConstitutionValidated",
        EventPayload::TemplateInstantiated { .. } => "TemplateInstantiated",
        EventPayload::TaskCreated { .. } => "TaskCreated",
        EventPayload::TaskUpdated { .. } => "TaskUpdated",
        EventPayload::TaskRuntimeConfigured { .. } => "TaskRuntimeConfigured",
        EventPayload::TaskRuntimeRoleConfigured { .. } => "TaskRuntimeRoleConfigured",
        EventPayload::TaskRuntimeCleared { .. } => "TaskRuntimeCleared",
        EventPayload::TaskRuntimeRoleCleared { .. } => "TaskRuntimeRoleCleared",
        EventPayload::TaskRunModeSet { .. } => "TaskRunModeSet",
        EventPayload::TaskClosed { .. } => "TaskClosed",
        EventPayload::TaskGraphCreated { .. } => "TaskGraphCreated",
        EventPayload::TaskAddedToGraph { .. } => "TaskAddedToGraph",
        EventPayload::DependencyAdded { .. } => "DependencyAdded",
        EventPayload::GraphTaskCheckAdded { .. } => "GraphTaskCheckAdded",
        EventPayload::ScopeAssigned { .. } => "ScopeAssigned",
        EventPayload::TaskGraphValidated { .. } => "TaskGraphValidated",
        EventPayload::TaskGraphLocked { .. } => "TaskGraphLocked",
        EventPayload::TaskFlowCreated { .. } => "TaskFlowCreated",
        EventPayload::TaskFlowDependencyAdded { .. } => "TaskFlowDependencyAdded",
        EventPayload::TaskFlowRunModeSet { .. } => "TaskFlowRunModeSet",
        EventPayload::TaskFlowRuntimeConfigured { .. } => "TaskFlowRuntimeConfigured",
        EventPayload::TaskFlowRuntimeCleared { .. } => "TaskFlowRuntimeCleared",
        EventPayload::TaskFlowStarted { .. } => "TaskFlowStarted",
        EventPayload::TaskFlowPaused { .. } => "TaskFlowPaused",
        EventPayload::TaskFlowResumed { .. } => "TaskFlowResumed",
        EventPayload::TaskFlowCompleted { .. } => "TaskFlowCompleted",
        EventPayload::TaskFlowAborted { .. } => "TaskFlowAborted",
        EventPayload::TaskReady { .. } => "TaskReady",
        EventPayload::TaskBlocked { .. } => "TaskBlocked",
        EventPayload::ScopeConflictDetected { .. } => "ScopeConflictDetected",
        EventPayload::TaskSchedulingDeferred { .. } => "TaskSchedulingDeferred",
        EventPayload::TaskExecutionStateChanged { .. } => "TaskExecutionStateChanged",
        EventPayload::TaskExecutionStarted { .. } => "TaskExecutionStarted",
        EventPayload::TaskExecutionSucceeded { .. } => "TaskExecutionSucceeded",
        EventPayload::TaskExecutionFailed { .. } => "TaskExecutionFailed",
        EventPayload::AttemptStarted { .. } => "AttemptStarted",
        EventPayload::BaselineCaptured { .. } => "BaselineCaptured",
        EventPayload::FileModified { .. } => "FileModified",
        EventPayload::DiffComputed { .. } => "DiffComputed",
        EventPayload::CheckStarted { .. } => "CheckStarted",
        EventPayload::CheckCompleted { .. } => "CheckCompleted",
        EventPayload::MergeCheckStarted { .. } => "MergeCheckStarted",
        EventPayload::MergeCheckCompleted { .. } => "MergeCheckCompleted",
        EventPayload::CheckpointDeclared { .. } => "CheckpointDeclared",
        EventPayload::CheckpointActivated { .. } => "CheckpointActivated",
        EventPayload::CheckpointCompleted { .. } => "CheckpointCompleted",
        EventPayload::AllCheckpointsCompleted { .. } => "AllCheckpointsCompleted",
        EventPayload::CheckpointCommitCreated { .. } => "CheckpointCommitCreated",
        EventPayload::ScopeValidated { .. } => "ScopeValidated",
        EventPayload::ScopeViolationDetected { .. } => "ScopeViolationDetected",
        EventPayload::RetryContextAssembled { .. } => "RetryContextAssembled",
        EventPayload::TaskRetryRequested { .. } => "TaskRetryRequested",
        EventPayload::TaskAborted { .. } => "TaskAborted",
        EventPayload::HumanOverride { .. } => "HumanOverride",
        EventPayload::MergePrepared { .. } => "MergePrepared",
        EventPayload::TaskExecutionFrozen { .. } => "TaskExecutionFrozen",
        EventPayload::TaskIntegratedIntoFlow { .. } => "TaskIntegratedIntoFlow",
        EventPayload::MergeConflictDetected { .. } => "MergeConflictDetected",
        EventPayload::FlowFrozenForMerge { .. } => "FlowFrozenForMerge",
        EventPayload::FlowIntegrationLockAcquired { .. } => "FlowIntegrationLockAcquired",
        EventPayload::MergeApproved { .. } => "MergeApproved",
        EventPayload::MergeCompleted { .. } => "MergeCompleted",
        EventPayload::WorktreeCleanupPerformed { .. } => "WorktreeCleanupPerformed",
        EventPayload::RuntimeStarted { .. } => "RuntimeStarted",
        EventPayload::RuntimeOutputChunk { .. } => "RuntimeOutputChunk",
        EventPayload::RuntimeInputProvided { .. } => "RuntimeInputProvided",
        EventPayload::RuntimeInterrupted { .. } => "RuntimeInterrupted",
        EventPayload::RuntimeExited { .. } => "RuntimeExited",
        EventPayload::RuntimeTerminated { .. } => "RuntimeTerminated",
        EventPayload::RuntimeErrorClassified { .. } => "RuntimeErrorClassified",
        EventPayload::RuntimeRecoveryScheduled { .. } => "RuntimeRecoveryScheduled",
        EventPayload::RuntimeFilesystemObserved { .. } => "RuntimeFilesystemObserved",
        EventPayload::RuntimeCommandObserved { .. } => "RuntimeCommandObserved",
        EventPayload::RuntimeToolCallObserved { .. } => "RuntimeToolCallObserved",
        EventPayload::RuntimeTodoSnapshotUpdated { .. } => "RuntimeTodoSnapshotUpdated",
        EventPayload::RuntimeNarrativeOutputObserved { .. } => "RuntimeNarrativeOutputObserved",
        EventPayload::Unknown => "Unknown",
    }
}

fn payload_category(payload: &EventPayload) -> &'static str {
    match payload {
        EventPayload::ErrorOccurred { .. } => "error",

        EventPayload::ProjectCreated { .. }
        | EventPayload::ProjectUpdated { .. }
        | EventPayload::ProjectRuntimeConfigured { .. }
        | EventPayload::ProjectRuntimeRoleConfigured { .. }
        | EventPayload::GlobalRuntimeConfigured { .. }
        | EventPayload::RepositoryAttached { .. }
        | EventPayload::RepositoryDetached { .. }
        | EventPayload::GovernanceProjectStorageInitialized { .. }
        | EventPayload::GovernanceArtifactUpserted { .. }
        | EventPayload::GovernanceArtifactDeleted { .. }
        | EventPayload::GovernanceAttachmentLifecycleUpdated { .. }
        | EventPayload::GovernanceStorageMigrated { .. }
        | EventPayload::ConstitutionInitialized { .. }
        | EventPayload::ConstitutionUpdated { .. }
        | EventPayload::ConstitutionValidated { .. }
        | EventPayload::TemplateInstantiated { .. } => "project",

        EventPayload::TaskCreated { .. }
        | EventPayload::TaskUpdated { .. }
        | EventPayload::TaskRuntimeConfigured { .. }
        | EventPayload::TaskRuntimeRoleConfigured { .. }
        | EventPayload::TaskRuntimeCleared { .. }
        | EventPayload::TaskRuntimeRoleCleared { .. }
        | EventPayload::TaskRunModeSet { .. }
        | EventPayload::TaskClosed { .. } => "task",

        EventPayload::TaskGraphCreated { .. }
        | EventPayload::TaskAddedToGraph { .. }
        | EventPayload::DependencyAdded { .. }
        | EventPayload::GraphTaskCheckAdded { .. }
        | EventPayload::ScopeAssigned { .. }
        | EventPayload::TaskGraphValidated { .. }
        | EventPayload::TaskGraphLocked { .. } => "graph",

        EventPayload::TaskFlowCreated { .. }
        | EventPayload::TaskFlowDependencyAdded { .. }
        | EventPayload::TaskFlowRunModeSet { .. }
        | EventPayload::TaskFlowRuntimeConfigured { .. }
        | EventPayload::TaskFlowRuntimeCleared { .. }
        | EventPayload::TaskFlowStarted { .. }
        | EventPayload::TaskFlowPaused { .. }
        | EventPayload::TaskFlowResumed { .. }
        | EventPayload::TaskFlowCompleted { .. }
        | EventPayload::TaskFlowAborted { .. }
        | EventPayload::WorktreeCleanupPerformed { .. } => "flow",

        EventPayload::TaskReady { .. }
        | EventPayload::TaskBlocked { .. }
        | EventPayload::ScopeConflictDetected { .. }
        | EventPayload::TaskSchedulingDeferred { .. }
        | EventPayload::TaskExecutionStateChanged { .. }
        | EventPayload::TaskExecutionStarted { .. }
        | EventPayload::TaskExecutionSucceeded { .. }
        | EventPayload::TaskExecutionFailed { .. }
        | EventPayload::AttemptStarted { .. }
        | EventPayload::CheckpointDeclared { .. }
        | EventPayload::CheckpointActivated { .. }
        | EventPayload::CheckpointCompleted { .. }
        | EventPayload::AllCheckpointsCompleted { .. }
        | EventPayload::RetryContextAssembled { .. }
        | EventPayload::TaskRetryRequested { .. }
        | EventPayload::TaskAborted { .. } => "execution",

        EventPayload::CheckStarted { .. }
        | EventPayload::CheckCompleted { .. }
        | EventPayload::HumanOverride { .. } => "verification",

        EventPayload::ScopeValidated { .. } | EventPayload::ScopeViolationDetected { .. } => {
            "scope"
        }

        EventPayload::MergePrepared { .. }
        | EventPayload::MergeCheckStarted { .. }
        | EventPayload::MergeCheckCompleted { .. }
        | EventPayload::TaskExecutionFrozen { .. }
        | EventPayload::TaskIntegratedIntoFlow { .. }
        | EventPayload::MergeConflictDetected { .. }
        | EventPayload::FlowFrozenForMerge { .. }
        | EventPayload::FlowIntegrationLockAcquired { .. }
        | EventPayload::MergeApproved { .. }
        | EventPayload::MergeCompleted { .. } => "merge",

        EventPayload::RuntimeStarted { .. }
        | EventPayload::RuntimeOutputChunk { .. }
        | EventPayload::RuntimeInputProvided { .. }
        | EventPayload::RuntimeInterrupted { .. }
        | EventPayload::RuntimeExited { .. }
        | EventPayload::RuntimeTerminated { .. }
        | EventPayload::RuntimeErrorClassified { .. }
        | EventPayload::RuntimeRecoveryScheduled { .. }
        | EventPayload::RuntimeFilesystemObserved { .. }
        | EventPayload::RuntimeCommandObserved { .. }
        | EventPayload::RuntimeToolCallObserved { .. }
        | EventPayload::RuntimeTodoSnapshotUpdated { .. }
        | EventPayload::RuntimeNarrativeOutputObserved { .. }
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
        tiny_http::Header::from_bytes(
            &b"Access-Control-Allow-Methods"[..],
            &b"GET, POST, OPTIONS"[..],
        )
        .expect("static header"),
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..])
            .expect("static header"),
    ]
}

fn parse_json_body<T: for<'de> Deserialize<'de>>(body: Option<&[u8]>, origin: &str) -> Result<T> {
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

fn parse_runtime_role(raw: Option<&str>, origin: &str) -> Result<RuntimeRole> {
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

fn parse_run_mode(raw: &str, origin: &str) -> Result<RunMode> {
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

fn parse_merge_mode(raw: Option<&str>, origin: &str) -> Result<MergeExecuteMode> {
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

fn env_pairs_from_map(env: Option<HashMap<String, String>>) -> Vec<String> {
    let mut env_pairs = Vec::new();
    if let Some(env) = env {
        for (k, v) in env {
            env_pairs.push(format!("{k}={v}"));
        }
    }
    env_pairs
}

fn method_not_allowed(path: &str, method: ApiMethod, allowed: &'static str) -> Result<ApiResponse> {
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

#[derive(Debug, Deserialize)]
struct ProjectCreateRequest {
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectUpdateRequest {
    project: String,
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectRuntimeRequest {
    project: String,
    role: Option<String>,
    adapter: Option<String>,
    binary_path: Option<String>,
    model: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
    max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ProjectAttachRepoRequest {
    project: String,
    path: String,
    name: Option<String>,
    access: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectDetachRepoRequest {
    project: String,
    repo_name: String,
}

#[derive(Debug, Deserialize)]
struct TaskCreateRequest {
    project: String,
    title: String,
    description: Option<String>,
    scope: Option<crate::core::scope::Scope>,
}

#[derive(Debug, Deserialize)]
struct TaskUpdateRequest {
    task_id: String,
    title: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskCloseRequest {
    task_id: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskIdRequest {
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct TaskRetryRequest {
    task_id: String,
    reset_count: Option<bool>,
    mode: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskAbortRequest {
    task_id: String,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TaskRuntimeSetRequest {
    task_id: String,
    clear: Option<bool>,
    role: Option<String>,
    adapter: Option<String>,
    binary_path: Option<String>,
    model: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TaskSetRunModeRequest {
    task_id: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
struct GraphCreateRequest {
    project: String,
    name: String,
    from_tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GraphDependencyRequest {
    graph_id: String,
    from_task: String,
    to_task: String,
}

#[derive(Debug, Deserialize)]
struct GraphAddCheckRequest {
    graph_id: String,
    task_id: String,
    name: String,
    command: String,
    required: Option<bool>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GraphValidateRequest {
    graph_id: String,
}

#[derive(Debug, Deserialize)]
struct FlowCreateRequest {
    graph_id: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FlowIdRequest {
    flow_id: String,
}

#[derive(Debug, Deserialize)]
struct FlowTickRequest {
    flow_id: String,
    interactive: Option<bool>,
    max_parallel: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct FlowAbortRequest {
    flow_id: String,
    reason: Option<String>,
    force: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct FlowSetRunModeRequest {
    flow_id: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
struct FlowAddDependencyRequest {
    flow_id: String,
    depends_on_flow_id: String,
}

#[derive(Debug, Deserialize)]
struct FlowRuntimeSetRequest {
    flow_id: String,
    clear: Option<bool>,
    role: Option<String>,
    adapter: Option<String>,
    binary_path: Option<String>,
    model: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
    max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct VerifyOverrideRequest {
    task_id: String,
    decision: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct VerifyRunRequest {
    task_id: String,
}

#[derive(Debug, Deserialize)]
struct MergePrepareRequest {
    flow_id: String,
    target: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MergeApproveRequest {
    flow_id: String,
}

#[derive(Debug, Deserialize)]
struct MergeExecuteRequest {
    flow_id: String,
    mode: Option<String>,
    monitor_ci: Option<bool>,
    auto_merge: Option<bool>,
    pull_after: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CheckpointCompleteRequest {
    attempt_id: String,
    checkpoint_id: String,
    summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorktreeCleanupRequest {
    flow_id: String,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct RuntimeDefaultsSetRequest {
    role: Option<String>,
    adapter: Option<String>,
    binary_path: Option<String>,
    model: Option<String>,
    args: Option<Vec<String>>,
    env: Option<HashMap<String, String>>,
    timeout_ms: Option<u64>,
    max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Serialize)]
struct VerifyResultsView {
    attempt_id: String,
    task_id: String,
    flow_id: String,
    attempt_number: u32,
    check_results: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct AttemptInspectView {
    attempt_id: String,
    task_id: String,
    flow_id: String,
    attempt_number: u32,
    started_at: DateTime<Utc>,
    baseline_id: Option<String>,
    diff_id: Option<String>,
    diff: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiCatalog {
    read_endpoints: Vec<&'static str>,
    write_endpoints: Vec<&'static str>,
}

fn api_catalog() -> ApiCatalog {
    ApiCatalog {
        read_endpoints: vec![
            "/api/version",
            "/api/state",
            "/api/projects",
            "/api/tasks",
            "/api/graphs",
            "/api/flows",
            "/api/merges",
            "/api/runtimes",
            "/api/runtimes/health?project=<id|name>&task=<id>&flow=<id>&role=worker|validator",
            "/api/events",
            "/api/events/inspect?event_id=<id>",
            "/api/verify/results?attempt_id=<id>&output=true|false",
            "/api/attempts/inspect?attempt_id=<id>&diff=true|false",
            "/api/attempts/diff?attempt_id=<id>",
            "/api/flows/replay?flow_id=<id>",
            "/api/worktrees?flow_id=<id>",
            "/api/worktrees/inspect?task_id=<id>",
        ],
        write_endpoints: vec![
            "/api/projects/create",
            "/api/projects/update",
            "/api/projects/runtime",
            "/api/runtime/defaults",
            "/api/projects/repos/attach",
            "/api/projects/repos/detach",
            "/api/tasks/create",
            "/api/tasks/update",
            "/api/tasks/runtime",
            "/api/tasks/run-mode",
            "/api/tasks/close",
            "/api/tasks/start",
            "/api/tasks/complete",
            "/api/tasks/retry",
            "/api/tasks/abort",
            "/api/graphs/create",
            "/api/graphs/dependencies/add",
            "/api/graphs/checks/add",
            "/api/graphs/validate",
            "/api/flows/create",
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
        ],
    }
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

#[allow(clippy::too_many_lines)]
fn handle_api_request_inner(
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

    match path {
        "/health" if method == ApiMethod::Get => {
            let mut resp = ApiResponse::text(200, "text/plain", "ok\n");
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/version" if method == ApiMethod::Get => {
            let value = serde_json::json!({"version": env!("CARGO_PKG_VERSION")});
            let wrapped = CliResponse::success(value);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/catalog" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(api_catalog());
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/state" if method == ApiMethod::Get => {
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
        "/api/projects" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.list_projects()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_tasks(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_graphs(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_flows(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merges" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_merge_states(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtimes" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.runtime_list());
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtimes/health" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let role = parse_runtime_role(
                query.get("role").map(String::as_str),
                "server:runtimes:health",
            )?;
            let wrapped = CliResponse::success(registry.runtime_health_with_role(
                query.get("project").map(String::as_str),
                query.get("task").map(String::as_str),
                query.get("flow").map(String::as_str),
                role,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/events" if method == ApiMethod::Get => {
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
        "/api/events/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let event_id = query.get("event_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_event_id",
                    "Query parameter 'event_id' is required",
                    "server:events:inspect",
                )
            })?;
            let wrapped = CliResponse::success(registry.get_event(event_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/results" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:verify:results",
                )
            })?;
            let output = query.get("output").is_some_and(|v| v == "true");
            let attempt = registry.get_attempt(attempt_id)?;
            let check_results = attempt
                .check_results
                .iter()
                .map(|r| {
                    if output {
                        serde_json::json!(r)
                    } else {
                        serde_json::json!({
                            "name": r.name,
                            "passed": r.passed,
                            "exit_code": r.exit_code,
                            "duration_ms": r.duration_ms,
                            "required": r.required,
                        })
                    }
                })
                .collect::<Vec<_>>();
            let view = VerifyResultsView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                check_results,
            };
            let wrapped = CliResponse::success(view);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/attempts/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:attempts:inspect",
                )
            })?;
            let include_diff = query.get("diff").is_some_and(|v| v == "true");
            let attempt = registry.get_attempt(attempt_id)?;
            let diff = if include_diff {
                registry.get_attempt_diff(attempt_id)?
            } else {
                None
            };
            let view = AttemptInspectView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                started_at: attempt.started_at,
                baseline_id: attempt.baseline_id.map(|v| v.to_string()),
                diff_id: attempt.diff_id.map(|v| v.to_string()),
                diff,
            };
            let wrapped = CliResponse::success(view);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/attempts/diff" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:attempts:diff",
                )
            })?;
            let wrapped = CliResponse::success(serde_json::json!({
                "attempt_id": attempt_id,
                "diff": registry.get_attempt_diff(attempt_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/replay" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:flows:replay",
                )
            })?;
            let wrapped = CliResponse::success(registry.replay_flow(flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:worktrees:list",
                )
            })?;
            let wrapped = CliResponse::success(registry.worktree_list(flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let task_id = query.get("task_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_task_id",
                    "Query parameter 'task_id' is required",
                    "server:worktrees:inspect",
                )
            })?;
            let wrapped = CliResponse::success(registry.worktree_inspect(task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/create" if method == ApiMethod::Post => {
            let req: ProjectCreateRequest = parse_json_body(body, "server:projects:create")?;
            let wrapped = CliResponse::success(
                registry.create_project(&req.name, req.description.as_deref())?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/update" if method == ApiMethod::Post => {
            let req: ProjectUpdateRequest = parse_json_body(body, "server:projects:update")?;
            let wrapped = CliResponse::success(registry.update_project(
                &req.project,
                req.name.as_deref(),
                req.description.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/runtime" if method == ApiMethod::Post => {
            let req: ProjectRuntimeRequest = parse_json_body(body, "server:projects:runtime")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:projects:runtime")?;
            let wrapped = CliResponse::success(registry.project_runtime_set_role(
                &req.project,
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtime/defaults" if method == ApiMethod::Post => {
            let req: RuntimeDefaultsSetRequest =
                parse_json_body(body, "server:runtime:defaults:set")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:runtime:defaults:set")?;
            registry.runtime_defaults_set(
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?;
            let wrapped = CliResponse::success(serde_json::json!({ "ok": true }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/repos/attach" if method == ApiMethod::Post => {
            let req: ProjectAttachRepoRequest =
                parse_json_body(body, "server:projects:repos:attach")?;
            let access_mode = match req
                .access
                .as_deref()
                .unwrap_or("rw")
                .to_lowercase()
                .as_str()
            {
                "ro" | "readonly" => crate::core::scope::RepoAccessMode::ReadOnly,
                "rw" | "readwrite" => crate::core::scope::RepoAccessMode::ReadWrite,
                other => {
                    return Err(HivemindError::user(
                        "invalid_access_mode",
                        format!("Invalid access mode '{other}'"),
                        "server:projects:repos:attach",
                    ));
                }
            };
            let wrapped = CliResponse::success(registry.attach_repo(
                &req.project,
                &req.path,
                req.name.as_deref(),
                access_mode,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/repos/detach" if method == ApiMethod::Post => {
            let req: ProjectDetachRepoRequest =
                parse_json_body(body, "server:projects:repos:detach")?;
            let wrapped = CliResponse::success(registry.detach_repo(&req.project, &req.repo_name)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/create" if method == ApiMethod::Post => {
            let req: TaskCreateRequest = parse_json_body(body, "server:tasks:create")?;
            let wrapped = CliResponse::success(registry.create_task(
                &req.project,
                &req.title,
                req.description.as_deref(),
                req.scope,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/update" if method == ApiMethod::Post => {
            let req: TaskUpdateRequest = parse_json_body(body, "server:tasks:update")?;
            let wrapped = CliResponse::success(registry.update_task(
                &req.task_id,
                req.title.as_deref(),
                req.description.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/close" if method == ApiMethod::Post => {
            let req: TaskCloseRequest = parse_json_body(body, "server:tasks:close")?;
            let wrapped =
                CliResponse::success(registry.close_task(&req.task_id, req.reason.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/start" if method == ApiMethod::Post => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:start")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "attempt_id": registry.start_task_execution(&req.task_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/complete" if method == ApiMethod::Post => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:complete")?;
            let wrapped = CliResponse::success(registry.complete_task_execution(&req.task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/retry" if method == ApiMethod::Post => {
            let req: TaskRetryRequest = parse_json_body(body, "server:tasks:retry")?;
            let mode = match req
                .mode
                .as_deref()
                .unwrap_or("clean")
                .to_lowercase()
                .as_str()
            {
                "clean" => RetryMode::Clean,
                "continue" => RetryMode::Continue,
                other => {
                    return Err(HivemindError::user(
                        "invalid_retry_mode",
                        format!("Invalid retry mode '{other}'"),
                        "server:tasks:retry",
                    ));
                }
            };
            let wrapped = CliResponse::success(registry.retry_task(
                &req.task_id,
                req.reset_count.unwrap_or(false),
                mode,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/abort" if method == ApiMethod::Post => {
            let req: TaskAbortRequest = parse_json_body(body, "server:tasks:abort")?;
            let wrapped =
                CliResponse::success(registry.abort_task(&req.task_id, req.reason.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/runtime" if method == ApiMethod::Post => {
            let req: TaskRuntimeSetRequest = parse_json_body(body, "server:tasks:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:tasks:runtime")?;
            let wrapped = if req.clear.unwrap_or(false) {
                CliResponse::success(registry.task_runtime_clear_role(&req.task_id, role)?)
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                CliResponse::success(registry.task_runtime_set_role(
                    &req.task_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                )?)
            };
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/run-mode" if method == ApiMethod::Post => {
            let req: TaskSetRunModeRequest = parse_json_body(body, "server:tasks:run-mode")?;
            let wrapped = CliResponse::success(registry.task_set_run_mode(
                &req.task_id,
                parse_run_mode(&req.mode, "server:tasks:run-mode")?,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/create" if method == ApiMethod::Post => {
            let req: GraphCreateRequest = parse_json_body(body, "server:graphs:create")?;
            let wrapped = CliResponse::success(
                registry.create_graph(
                    &req.project,
                    &req.name,
                    &req.from_tasks
                        .iter()
                        .map(|s| {
                            uuid::Uuid::parse_str(s).map_err(|_| {
                                HivemindError::user(
                                    "invalid_task_id",
                                    format!("'{s}' is not a valid task ID"),
                                    "server:graphs:create",
                                )
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                )?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/dependencies/add" if method == ApiMethod::Post => {
            let req: GraphDependencyRequest =
                parse_json_body(body, "server:graphs:dependencies:add")?;
            let wrapped = CliResponse::success(registry.add_graph_dependency(
                &req.graph_id,
                &req.from_task,
                &req.to_task,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/checks/add" if method == ApiMethod::Post => {
            let req: GraphAddCheckRequest = parse_json_body(body, "server:graphs:checks:add")?;
            let mut check = CheckConfig::new(req.name, req.command);
            check.required = req.required.unwrap_or(true);
            check.timeout_ms = req.timeout_ms;
            let wrapped = CliResponse::success(registry.add_graph_task_check(
                &req.graph_id,
                &req.task_id,
                check,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/validate" if method == ApiMethod::Post => {
            let req: GraphValidateRequest = parse_json_body(body, "server:graphs:validate")?;
            let wrapped = CliResponse::success(registry.validate_graph(&req.graph_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/create" if method == ApiMethod::Post => {
            let req: FlowCreateRequest = parse_json_body(body, "server:flows:create")?;
            let wrapped =
                CliResponse::success(registry.create_flow(&req.graph_id, req.name.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/start" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:start")?;
            let wrapped = CliResponse::success(registry.start_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/tick" if method == ApiMethod::Post => {
            let req: FlowTickRequest = parse_json_body(body, "server:flows:tick")?;
            let wrapped = CliResponse::success(registry.tick_flow(
                &req.flow_id,
                req.interactive.unwrap_or(false),
                req.max_parallel,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/pause" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:pause")?;
            let wrapped = CliResponse::success(registry.pause_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/resume" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:resume")?;
            let wrapped = CliResponse::success(registry.resume_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/abort" if method == ApiMethod::Post => {
            let req: FlowAbortRequest = parse_json_body(body, "server:flows:abort")?;
            let wrapped = CliResponse::success(registry.abort_flow(
                &req.flow_id,
                req.reason.as_deref(),
                req.force.unwrap_or(false),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/run-mode" if method == ApiMethod::Post => {
            let req: FlowSetRunModeRequest = parse_json_body(body, "server:flows:run-mode")?;
            let wrapped = CliResponse::success(registry.flow_set_run_mode(
                &req.flow_id,
                parse_run_mode(&req.mode, "server:flows:run-mode")?,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/dependencies/add" if method == ApiMethod::Post => {
            let req: FlowAddDependencyRequest =
                parse_json_body(body, "server:flows:dependencies:add")?;
            let wrapped = CliResponse::success(
                registry.flow_add_dependency(&req.flow_id, &req.depends_on_flow_id)?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/runtime" if method == ApiMethod::Post => {
            let req: FlowRuntimeSetRequest = parse_json_body(body, "server:flows:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:flows:runtime")?;
            let wrapped = if req.clear.unwrap_or(false) {
                CliResponse::success(registry.flow_runtime_clear(&req.flow_id, role)?)
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                CliResponse::success(registry.flow_runtime_set(
                    &req.flow_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                    req.max_parallel_tasks.unwrap_or(1),
                )?)
            };
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/override" if method == ApiMethod::Post => {
            let req: VerifyOverrideRequest = parse_json_body(body, "server:verify:override")?;
            let wrapped = CliResponse::success(registry.verify_override(
                &req.task_id,
                &req.decision,
                &req.reason,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/run" if method == ApiMethod::Post => {
            let req: VerifyRunRequest = parse_json_body(body, "server:verify:run")?;
            let wrapped = CliResponse::success(registry.verify_run(&req.task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/prepare" if method == ApiMethod::Post => {
            let req: MergePrepareRequest = parse_json_body(body, "server:merge:prepare")?;
            let wrapped =
                CliResponse::success(registry.merge_prepare(&req.flow_id, req.target.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/approve" if method == ApiMethod::Post => {
            let req: MergeApproveRequest = parse_json_body(body, "server:merge:approve")?;
            let wrapped = CliResponse::success(registry.merge_approve(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/execute" if method == ApiMethod::Post => {
            let req: MergeExecuteRequest = parse_json_body(body, "server:merge:execute")?;
            let wrapped = CliResponse::success(registry.merge_execute_with_options(
                &req.flow_id,
                MergeExecuteOptions {
                    mode: parse_merge_mode(req.mode.as_deref(), "server:merge:execute")?,
                    monitor_ci: req.monitor_ci.unwrap_or(false),
                    auto_merge: req.auto_merge.unwrap_or(false),
                    pull_after: req.pull_after.unwrap_or(false),
                },
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/checkpoints/complete" if method == ApiMethod::Post => {
            let req: CheckpointCompleteRequest =
                parse_json_body(body, "server:checkpoints:complete")?;
            let wrapped = CliResponse::success(registry.checkpoint_complete(
                &req.attempt_id,
                &req.checkpoint_id,
                req.summary.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees/cleanup" if method == ApiMethod::Post => {
            let req: WorktreeCleanupRequest = parse_json_body(body, "server:worktrees:cleanup")?;
            let result = registry.worktree_cleanup(&req.flow_id, req.force, req.dry_run)?;
            let wrapped = CliResponse::success(result);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/health"
        | "/api/version"
        | "/api/catalog"
        | "/api/state"
        | "/api/projects"
        | "/api/tasks"
        | "/api/graphs"
        | "/api/flows"
        | "/api/merges"
        | "/api/runtimes"
        | "/api/runtimes/health"
        | "/api/events"
        | "/api/events/inspect"
        | "/api/verify/results"
        | "/api/attempts/inspect"
        | "/api/attempts/diff"
        | "/api/flows/replay"
        | "/api/worktrees"
        | "/api/worktrees/inspect" => method_not_allowed(path, method, "GET"),
        "/api/projects/create"
        | "/api/projects/update"
        | "/api/projects/runtime"
        | "/api/runtime/defaults"
        | "/api/projects/repos/attach"
        | "/api/projects/repos/detach"
        | "/api/tasks/create"
        | "/api/tasks/update"
        | "/api/tasks/runtime"
        | "/api/tasks/run-mode"
        | "/api/tasks/close"
        | "/api/tasks/start"
        | "/api/tasks/complete"
        | "/api/tasks/retry"
        | "/api/tasks/abort"
        | "/api/graphs/create"
        | "/api/graphs/dependencies/add"
        | "/api/graphs/checks/add"
        | "/api/graphs/validate"
        | "/api/flows/create"
        | "/api/flows/start"
        | "/api/flows/tick"
        | "/api/flows/pause"
        | "/api/flows/resume"
        | "/api/flows/abort"
        | "/api/flows/run-mode"
        | "/api/flows/dependencies/add"
        | "/api/flows/runtime"
        | "/api/verify/override"
        | "/api/verify/run"
        | "/api/merge/prepare"
        | "/api/merge/approve"
        | "/api/merge/execute"
        | "/api/checkpoints/complete"
        | "/api/worktrees/cleanup" => method_not_allowed(path, method, "POST"),
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

    for mut req in server.incoming_requests() {
        let Some(method) = ApiMethod::from_http(req.method()) else {
            let _ = req.respond(tiny_http::Response::empty(405));
            continue;
        };

        let mut request_body = Vec::new();
        if method == ApiMethod::Post {
            let _ = req.as_reader().read_to_end(&mut request_body);
        }

        let response = match handle_api_request(
            method,
            req.url(),
            config.events_limit,
            if request_body.is_empty() {
                None
            } else {
                Some(request_body.as_slice())
            },
        ) {
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
    use std::mem;

    fn test_registry() -> Registry {
        let tmp = tempfile::tempdir().expect("tempdir");
        let data_dir = tmp.path().to_path_buf();
        mem::forget(tmp);
        let config = RegistryConfig::with_dir(data_dir);
        Registry::open_with_config(config).expect("registry")
    }

    fn json_value(body: &[u8]) -> Value {
        serde_json::from_slice(body).expect("json")
    }

    #[test]
    fn api_version_ok() {
        let reg = test_registry();
        let resp =
            handle_api_request_inner(ApiMethod::Get, "/api/version", 10, None, &reg).unwrap();
        assert_eq!(resp.status_code, 200);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], true);
        assert!(v["data"]["version"].is_string());
    }

    #[test]
    fn api_state_ok_empty() {
        let reg = test_registry();
        let resp = handle_api_request_inner(ApiMethod::Get, "/api/state", 10, None, &reg).unwrap();
        assert_eq!(resp.status_code, 200);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], true);
        assert!(v["data"]["projects"].is_array());
    }

    #[test]
    fn api_unknown_endpoint_404() {
        let reg = test_registry();
        let resp = handle_api_request_inner(ApiMethod::Get, "/api/nope", 10, None, &reg).unwrap();
        assert_eq!(resp.status_code, 404);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], false);
        assert_eq!(v["error"]["code"], "endpoint_not_found");
    }

    #[test]
    fn api_post_project_create_ok() {
        let reg = test_registry();
        let body = serde_json::json!({
            "name": "proj-a",
            "description": "project from api"
        });
        let body = serde_json::to_vec(&body).expect("json body");
        let resp = handle_api_request_inner(
            ApiMethod::Post,
            "/api/projects/create",
            10,
            Some(&body),
            &reg,
        )
        .unwrap();
        assert_eq!(resp.status_code, 200);
        let v = json_value(&resp.body);
        assert_eq!(v["success"], true);
        assert_eq!(v["data"]["name"], "proj-a");
    }
}
