use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectCreateRequest {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectUpdateRequest {
    pub(crate) project: String,
    pub(crate) name: Option<String>,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectDeleteRequest {
    pub(crate) project: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectRuntimeRequest {
    pub(crate) project: String,
    pub(crate) role: Option<String>,
    pub(crate) adapter: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<HashMap<String, String>>,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) max_parallel_tasks: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectAttachRepoRequest {
    pub(crate) project: String,
    pub(crate) path: String,
    pub(crate) name: Option<String>,
    pub(crate) access: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectDetachRepoRequest {
    pub(crate) project: String,
    pub(crate) repo_name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RuntimeDefaultsSetRequest {
    pub(crate) role: Option<String>,
    pub(crate) adapter: Option<String>,
    pub(crate) binary_path: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) args: Option<Vec<String>>,
    pub(crate) env: Option<HashMap<String, String>>,
    pub(crate) timeout_ms: Option<u64>,
    pub(crate) max_parallel_tasks: Option<u16>,
}
