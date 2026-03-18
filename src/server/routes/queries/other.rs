use super::*;

pub(super) fn handle_other_queries(
    path: &str,
    url: &str,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let event_service = || app.event_service();
    let worktree_service = || app.worktree_service();

    let resp = match path {
        "/api/flows/replay" => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:flows:replay",
                )
            })?;
            super::json_ok(event_service()?.replay_flow(flow_id)?)?
        }
        "/api/worktrees" => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:worktrees:list",
                )
            })?;
            super::json_ok(worktree_service()?.worktree_list(flow_id)?)?
        }
        "/api/worktrees/inspect" => {
            let query = parse_query(url);
            let task_id = query.get("task_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_task_id",
                    "Query parameter 'task_id' is required",
                    "server:worktrees:inspect",
                )
            })?;
            super::json_ok(worktree_service()?.worktree_inspect(task_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
