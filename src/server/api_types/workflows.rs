use super::*;
use crate::core::workflow::{WorkflowStepKind, WorkflowStepState};

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowCreateRequest {
    pub(crate) project: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowUpdateRequest {
    pub(crate) workflow_id: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) clear_description: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowStepAddRequest {
    pub(crate) workflow_id: String,
    pub(crate) name: String,
    pub(crate) kind: WorkflowStepKind,
    pub(crate) description: Option<String>,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowRunIdRequest {
    pub(crate) workflow_run_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowTickRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) interactive: Option<bool>,
    pub(crate) max_parallel: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowRunCreateRequest {
    pub(crate) workflow_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowAbortRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) reason: Option<String>,
    pub(crate) force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowStepStateRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) step_id: String,
    pub(crate) state: WorkflowStepState,
    pub(crate) reason: Option<String>,
}
