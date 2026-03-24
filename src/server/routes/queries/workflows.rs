use super::*;

pub(super) fn handle_workflow_queries(
    path: &str,
    url: &str,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/workflows" => {
            let query = parse_query(url);
            let registry = app.open_registry()?;
            let project = query.get("project").map(String::as_str);
            super::json_ok(registry.list_workflows(project)?)?
        }
        "/api/workflows/inspect" => {
            let query = parse_query(url);
            let workflow_id = query.get("workflow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_workflow_id",
                    "Query parameter 'workflow_id' is required",
                    "server:workflows:inspect",
                )
            })?;
            let registry = app.open_registry()?;
            super::json_ok(registry.get_workflow(workflow_id)?)?
        }
        "/api/workflow-runs" => {
            let query = parse_query(url);
            let registry = app.open_registry()?;
            let project = query.get("project").map(String::as_str);
            let workflow = query.get("workflow").map(String::as_str);
            super::json_ok(registry.list_workflow_runs(project, workflow)?)?
        }
        "/api/workflow-runs/inspect" => {
            let query = parse_query(url);
            let run_id = query.get("workflow_run_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_workflow_run_id",
                    "Query parameter 'workflow_run_id' is required",
                    "server:workflow-runs:inspect",
                )
            })?;
            let registry = app.open_registry()?;
            super::json_ok(registry.inspect_workflow_run(run_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
