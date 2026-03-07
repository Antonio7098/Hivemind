use super::*;

pub(crate) const fn default_max_parallel_tasks() -> u16 {
    1
}
pub(crate) const fn default_runtime_role_worker() -> RuntimeRole {
    RuntimeRole::Worker
}
/// Runtime role for model/runtime defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeRole {
    Worker,
    Validator,
}
/// Source used to resolve an effective runtime configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSelectionSource {
    TaskOverride,
    FlowDefault,
    ProjectDefault,
    GlobalDefault,
}
impl RuntimeSelectionSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TaskOverride => "task_override",
            Self::FlowDefault => "flow_default",
            Self::ProjectDefault => "project_default",
            Self::GlobalDefault => "global_default",
        }
    }
}
/// Correlation identifiers embedded in native runtime event payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeEventCorrelation {
    pub project_id: Uuid,
    pub graph_id: Uuid,
    pub flow_id: Uuid,
    pub task_id: Uuid,
    pub attempt_id: Uuid,
}
/// Payload capture mode for native runtime event payload fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeEventPayloadCaptureMode {
    MetadataOnly,
    FullPayload,
}
/// Hash-addressed payload blob metadata used by native runtime events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeBlobRef {
    pub digest: String,
    pub byte_size: u64,
    pub media_type: String,
    pub blob_path: String,
    #[serde(default)]
    pub payload: Option<String>,
}
/// Output stream for runtime output events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeOutputStream {
    Stdout,
    Stderr,
}
