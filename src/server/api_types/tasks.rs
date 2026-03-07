use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct TaskCreateRequest {
    pub(crate) project: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) scope: Option<crate::core::scope::Scope>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskUpdateRequest {
    pub(crate) task_id: String,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskDeleteRequest {
    pub(crate) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskCloseRequest {
    pub(crate) task_id: String,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskIdRequest {
    pub(crate) task_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskRetryRequest {
    pub(crate) task_id: String,
    pub(crate) reset_count: Option<bool>,
    pub(crate) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskAbortRequest {
    pub(crate) task_id: String,
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskRuntimeSetRequest {
    pub(crate) task_id: String,
    pub(crate) clear: Option<bool>,
    pub(crate) role: Option<String>,
    pub(crate) adapter: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<HashMap<String, String>>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskSetRunModeRequest {
    pub(crate) task_id: String,
    pub(crate) mode: String,
}
