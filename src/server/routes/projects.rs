use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let project_service = || app.project_service();
    let runtime_service = || app.runtime_service();
    let resp = match path {
        "/api/projects/create" => {
            let req: ProjectCreateRequest = parse_json_body(body, "server:projects:create")?;
            super::json_ok(
                project_service()?.create_project(&req.name, req.description.as_deref())?,
            )?
        }
        "/api/projects/update" => {
            let req: ProjectUpdateRequest = parse_json_body(body, "server:projects:update")?;
            super::json_ok(project_service()?.update_project(
                &req.project,
                req.name.as_deref(),
                req.description.as_deref(),
            )?)?
        }
        "/api/projects/delete" => {
            let req: ProjectDeleteRequest = parse_json_body(body, "server:projects:delete")?;
            super::json_ok(serde_json::json!({
                "project_id": project_service()?.delete_project(&req.project)?,
            }))?
        }
        "/api/projects/runtime" => {
            let req: ProjectRuntimeRequest = parse_json_body(body, "server:projects:runtime")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:projects:runtime")?;
            super::json_ok(project_service()?.project_runtime_set_role(
                &req.project,
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?)?
        }
        "/api/runtime/defaults" => {
            let req: RuntimeDefaultsSetRequest =
                parse_json_body(body, "server:runtime:defaults:set")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:runtime:defaults:set")?;
            runtime_service()?.runtime_defaults_set(
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?;
            super::json_ok(serde_json::json!({ "ok": true }))?
        }
        "/api/projects/repos/attach" => {
            let req: ProjectAttachRepoRequest =
                parse_json_body(body, "server:projects:repos:attach")?;
            super::json_ok(project_service()?.attach_repo(
                &req.project,
                &req.path,
                req.name.as_deref(),
                parse_access_mode(req.access.as_deref())?,
            )?)?
        }
        "/api/projects/repos/detach" => {
            let req: ProjectDetachRepoRequest =
                parse_json_body(body, "server:projects:repos:detach")?;
            super::json_ok(project_service()?.detach_repo(&req.project, &req.repo_name)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

fn parse_access_mode(access: Option<&str>) -> Result<crate::core::scope::RepoAccessMode> {
    match access.unwrap_or("rw").to_lowercase().as_str() {
        "ro" | "readonly" => Ok(crate::core::scope::RepoAccessMode::ReadOnly),
        "rw" | "readwrite" => Ok(crate::core::scope::RepoAccessMode::ReadWrite),
        other => Err(HivemindError::user(
            "invalid_access_mode",
            format!("Invalid access mode '{other}'"),
            "server:projects:repos:attach",
        )),
    }
}
