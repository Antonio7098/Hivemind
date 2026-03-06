use super::*;

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectIdRequest {
    pub(crate) project: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GraphSnapshotRefreshRequest {
    pub(crate) project: String,
    pub(crate) trigger: Option<String>,
}
