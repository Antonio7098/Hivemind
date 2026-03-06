use super::*;

#[derive(Debug, Deserialize)]
pub(super) struct ProjectCreateRequest {
    pub(super) name: String,
    pub(super) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectUpdateRequest {
    pub(super) project: String,
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectDeleteRequest {
    pub(super) project: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectRuntimeRequest {
    pub(super) project: String,
    pub(super) role: Option<String>,
    pub(super) adapter: Option<String>,
    pub(super) binary_path: Option<String>,
    pub(super) model: Option<String>,
    pub(super) args: Option<Vec<String>>,
    pub(super) env: Option<HashMap<String, String>>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectAttachRepoRequest {
    pub(super) project: String,
    pub(super) path: String,
    pub(super) name: Option<String>,
    pub(super) access: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectDetachRepoRequest {
    pub(super) project: String,
    pub(super) repo_name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskCreateRequest {
    pub(super) project: String,
    pub(super) title: String,
    pub(super) description: Option<String>,
    pub(super) scope: Option<crate::core::scope::Scope>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskUpdateRequest {
    pub(super) task_id: String,
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskDeleteRequest {
    pub(super) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskCloseRequest {
    pub(super) task_id: String,
    pub(super) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskIdRequest {
    pub(super) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskRetryRequest {
    pub(super) task_id: String,
    pub(super) reset_count: Option<bool>,
    pub(super) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskAbortRequest {
    pub(super) task_id: String,
    pub(super) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskRuntimeSetRequest {
    pub(super) task_id: String,
    pub(super) clear: Option<bool>,
    pub(super) role: Option<String>,
    pub(super) adapter: Option<String>,
    pub(super) binary_path: Option<String>,
    pub(super) model: Option<String>,
    pub(super) args: Option<Vec<String>>,
    pub(super) env: Option<HashMap<String, String>>,
    pub(super) timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TaskSetRunModeRequest {
    pub(super) task_id: String,
    pub(super) mode: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphCreateRequest {
    pub(super) project: String,
    pub(super) name: String,
    pub(super) from_tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphDependencyRequest {
    pub(super) graph_id: String,
    pub(super) from_task: String,
    pub(super) to_task: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphAddCheckRequest {
    pub(super) graph_id: String,
    pub(super) task_id: String,
    pub(super) name: String,
    pub(super) command: String,
    pub(super) required: Option<bool>,
    pub(super) timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphValidateRequest {
    pub(super) graph_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphDeleteRequest {
    pub(super) graph_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowCreateRequest {
    pub(super) graph_id: String,
    pub(super) name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowIdRequest {
    pub(super) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowDeleteRequest {
    pub(super) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowTickRequest {
    pub(super) flow_id: String,
    pub(super) interactive: Option<bool>,
    pub(super) max_parallel: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowAbortRequest {
    pub(super) flow_id: String,
    pub(super) reason: Option<String>,
    pub(super) force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowSetRunModeRequest {
    pub(super) flow_id: String,
    pub(super) mode: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowAddDependencyRequest {
    pub(super) flow_id: String,
    pub(super) depends_on_flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct FlowRuntimeSetRequest {
    pub(super) flow_id: String,
    pub(super) clear: Option<bool>,
    pub(super) role: Option<String>,
    pub(super) adapter: Option<String>,
    pub(super) binary_path: Option<String>,
    pub(super) model: Option<String>,
    pub(super) args: Option<Vec<String>>,
    pub(super) env: Option<HashMap<String, String>>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct VerifyOverrideRequest {
    pub(super) task_id: String,
    pub(super) decision: String,
    pub(super) reason: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct VerifyRunRequest {
    pub(super) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct MergePrepareRequest {
    pub(super) flow_id: String,
    pub(super) target: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MergeApproveRequest {
    pub(super) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct MergeExecuteRequest {
    pub(super) flow_id: String,
    pub(super) mode: Option<String>,
    pub(super) monitor_ci: Option<bool>,
    pub(super) auto_merge: Option<bool>,
    pub(super) pull_after: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CheckpointCompleteRequest {
    pub(super) attempt_id: String,
    pub(super) checkpoint_id: String,
    pub(super) summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WorktreeCleanupRequest {
    pub(super) flow_id: String,
    #[serde(default)]
    pub(super) force: bool,
    #[serde(default)]
    pub(super) dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub(super) struct RuntimeDefaultsSetRequest {
    pub(super) role: Option<String>,
    pub(super) adapter: Option<String>,
    pub(super) binary_path: Option<String>,
    pub(super) model: Option<String>,
    pub(super) args: Option<Vec<String>>,
    pub(super) env: Option<HashMap<String, String>>,
    pub(super) timeout_ms: Option<u64>,
    pub(super) max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProjectIdRequest {
    pub(super) project: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct GraphSnapshotRefreshRequest {
    pub(super) project: String,
    pub(super) trigger: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct VerifyResultsView {
    pub(super) attempt_id: String,
    pub(super) task_id: String,
    pub(super) flow_id: String,
    pub(super) attempt_number: u32,
    pub(super) check_results: Vec<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct AttemptInspectView {
    pub(super) attempt_id: String,
    pub(super) task_id: String,
    pub(super) flow_id: String,
    pub(super) attempt_number: u32,
    pub(super) started_at: DateTime<Utc>,
    pub(super) baseline_id: Option<String>,
    pub(super) diff_id: Option<String>,
    pub(super) diff: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ApiCatalog {
    pub(super) read_endpoints: Vec<&'static str>,
    pub(super) write_endpoints: Vec<&'static str>,
}

pub(super) fn api_catalog() -> ApiCatalog {
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
            "/api/governance/constitution?project=<id|name>",
            "/api/governance/documents?project=<id|name>",
            "/api/governance/documents/inspect?project=<id|name>&document_id=<id>",
            "/api/governance/notepad?project=<id|name>",
            "/api/governance/global/notepad",
            "/api/governance/global/skills",
            "/api/governance/global/skills/inspect?skill_id=<id>",
            "/api/governance/global/templates",
            "/api/governance/global/templates/inspect?template_id=<id>",
        ],
        write_endpoints: vec![
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
            "/api/governance/constitution/check",
            "/api/governance/graph-snapshot/refresh",
        ],
    }
}
