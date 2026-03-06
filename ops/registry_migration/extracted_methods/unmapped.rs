// AUTO-GENERATED from src/core/registry_original.rs
// Module bucket: unmapped

// project_for_flow (6115-6123)
    fn project_for_flow<'a>(flow: &TaskFlow, state: &'a AppState) -> Result<&'a Project> {
        state.projects.get(&flow.project_id).ok_or_else(|| {
            HivemindError::system(
                "project_not_found",
                format!("Project '{}' not found", flow.project_id),
                "registry:worktree_manager_for_flow",
            )
        })
    }

