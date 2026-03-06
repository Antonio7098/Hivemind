use super::*;

pub(super) fn handle_get(
    path: &str,
    url: &str,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/governance/constitution" => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            super::json_ok(registry.constitution_show(project)?)?
        }
        "/api/governance/documents" => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            super::json_ok(registry.project_governance_document_list(project)?)?
        }
        "/api/governance/documents/inspect" => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            let document_id = query.get("document_id").map_or("", |s| s.as_str());
            super::json_ok(registry.project_governance_document_inspect(project, document_id)?)?
        }
        "/api/governance/notepad" => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            super::json_ok(registry.project_governance_notepad_show(project)?)?
        }
        "/api/governance/global/notepad" => super::json_ok(registry.global_notepad_show()?)?,
        "/api/governance/global/skills" => super::json_ok(registry.global_skill_list()?)?,
        "/api/governance/global/skills/inspect" => {
            let query = parse_query(url);
            let skill_id = query.get("skill_id").map_or("", |s| s.as_str());
            super::json_ok(registry.global_skill_inspect(skill_id)?)?
        }
        "/api/governance/global/templates" => super::json_ok(registry.global_template_list()?)?,
        "/api/governance/global/templates/inspect" => {
            let query = parse_query(url);
            let template_id = query.get("template_id").map_or("", |s| s.as_str());
            super::json_ok(registry.global_template_inspect(template_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp =
        match path {
            "/api/governance/constitution/check" => {
                let req: ProjectIdRequest =
                    parse_json_body(body, "server:governance:constitution:check")?;
                super::json_ok(registry.constitution_check(&req.project)?)?
            }
            "/api/governance/graph-snapshot/refresh" => {
                let req: GraphSnapshotRefreshRequest =
                    parse_json_body(body, "server:governance:graph-snapshot:refresh")?;
                super::json_ok(registry.graph_snapshot_refresh(
                    &req.project,
                    req.trigger.as_deref().unwrap_or("api"),
                )?)?
            }
            _ => return Ok(None),
        };

    Ok(Some(resp))
}
