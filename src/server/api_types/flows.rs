use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct FlowCreateRequest {
    pub(crate) graph_id: String,
    pub(crate) name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowIdRequest {
    pub(crate) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowDeleteRequest {
    pub(crate) flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowTickRequest {
    pub(crate) flow_id: String,
    pub(crate) interactive: Option<bool>,
    pub(crate) max_parallel: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowAbortRequest {
    pub(crate) flow_id: String,
    pub(crate) reason: Option<String>,
    pub(crate) force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowSetRunModeRequest {
    pub(crate) flow_id: String,
    pub(crate) mode: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowAddDependencyRequest {
    pub(crate) flow_id: String,
    pub(crate) depends_on_flow_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FlowRuntimeSetRequest {
    pub(crate) flow_id: String,
    pub(crate) clear: Option<bool>,
    pub(crate) role: Option<String>,
    pub(crate) adapter: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<HashMap<String, String>>,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) max_parallel_tasks: Option<u16>,
}
