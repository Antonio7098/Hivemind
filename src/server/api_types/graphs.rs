use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct GraphCreateRequest {
    pub(crate) project: String,
    pub(crate) name: String,
    pub(crate) from_tasks: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphDependencyRequest {
    pub(crate) graph_id: String,
    pub(crate) from_task: String,
    pub(crate) to_task: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphAddCheckRequest {
    pub(crate) graph_id: String,
    pub(crate) task_id: String,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) required: Option<bool>,
    pub(crate) timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphValidateRequest {
    pub(crate) graph_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphDeleteRequest {
    pub(crate) graph_id: String,
}
