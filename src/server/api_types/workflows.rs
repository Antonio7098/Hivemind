use super::*;
use crate::core::workflow::{
    WorkflowConditionalConfig, WorkflowContextPatchBinding, WorkflowDataValue,
    WorkflowStepInputBinding, WorkflowStepKind, WorkflowStepOutputBinding, WorkflowStepState,
    WorkflowWaitConfig,
};
use std::collections::BTreeMap;

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
    #[serde(default)]
    pub(crate) input_bindings: Vec<WorkflowStepInputBinding>,
    #[serde(default)]
    pub(crate) output_bindings: Vec<WorkflowStepOutputBinding>,
    #[serde(default)]
    pub(crate) context_patches: Vec<WorkflowContextPatchBinding>,
    pub(crate) child_workflow_id: Option<String>,
    pub(crate) conditional: Option<WorkflowConditionalConfig>,
    pub(crate) wait: Option<WorkflowWaitConfig>,
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
    pub(crate) context_schema: Option<String>,
    pub(crate) context_schema_version: Option<u32>,
    #[serde(default)]
    pub(crate) context_inputs: BTreeMap<String, WorkflowDataValue>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowAbortRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) reason: Option<String>,
    pub(crate) force: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowSignalRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) signal_name: String,
    pub(crate) idempotency_key: String,
    pub(crate) payload: Option<WorkflowDataValue>,
    pub(crate) step_id: Option<String>,
    pub(crate) emitted_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowStepStateRequest {
    pub(crate) workflow_run_id: String,
    pub(crate) step_id: String,
    pub(crate) state: WorkflowStepState,
    pub(crate) reason: Option<String>,
}
