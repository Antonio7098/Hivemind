use super::*;

pub(super) fn handle_post(
    path: &str,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/api/workflows/create" => {
            let req: WorkflowCreateRequest = parse_json_body(body, "server:workflows:create")?;
            super::json_ok(registry.create_workflow(
                &req.project,
                &req.name,
                req.description.as_deref(),
            )?)?
        }
        "/api/workflows/update" => {
            let req: WorkflowUpdateRequest = parse_json_body(body, "server:workflows:update")?;
            super::json_ok(registry.update_workflow(
                &req.workflow_id,
                req.name.as_deref(),
                req.description.as_deref(),
                req.clear_description.unwrap_or(false),
            )?)?
        }
        "/api/workflows/steps/add" => {
            let req: WorkflowStepAddRequest = parse_json_body(body, "server:workflows:steps:add")?;
            super::json_ok(registry.workflow_add_step(
                &req.workflow_id,
                &req.name,
                req.kind,
                req.description.as_deref(),
                &req.depends_on,
            )?)?
        }
        "/api/workflow-runs/create" => {
            let req: WorkflowRunCreateRequest =
                parse_json_body(body, "server:workflow-runs:create")?;
            super::json_ok(registry.create_workflow_run(&req.workflow_id)?)?
        }
        "/api/workflow-runs/start" => {
            let req: WorkflowRunIdRequest = parse_json_body(body, "server:workflow-runs:start")?;
            super::json_ok(registry.start_workflow_run(&req.workflow_run_id)?)?
        }
        "/api/workflow-runs/tick" => {
            let req: WorkflowTickRequest = parse_json_body(body, "server:workflow-runs:tick")?;
            super::json_ok(registry.tick_workflow_run(
                &req.workflow_run_id,
                req.interactive.unwrap_or(false),
                req.max_parallel,
            )?)?
        }
        "/api/workflow-runs/complete" => {
            let req: WorkflowRunIdRequest = parse_json_body(body, "server:workflow-runs:complete")?;
            super::json_ok(registry.complete_workflow_run(&req.workflow_run_id)?)?
        }
        "/api/workflow-runs/pause" => {
            let req: WorkflowRunIdRequest = parse_json_body(body, "server:workflow-runs:pause")?;
            super::json_ok(registry.pause_workflow_run(&req.workflow_run_id)?)?
        }
        "/api/workflow-runs/resume" => {
            let req: WorkflowRunIdRequest = parse_json_body(body, "server:workflow-runs:resume")?;
            super::json_ok(registry.resume_workflow_run(&req.workflow_run_id)?)?
        }
        "/api/workflow-runs/abort" => {
            let req: WorkflowAbortRequest = parse_json_body(body, "server:workflow-runs:abort")?;
            super::json_ok(registry.abort_workflow_run(
                &req.workflow_run_id,
                req.reason.as_deref(),
                req.force.unwrap_or(false),
            )?)?
        }
        "/api/workflow-runs/steps/state" => {
            let req: WorkflowStepStateRequest =
                parse_json_body(body, "server:workflow-runs:steps:state")?;
            super::json_ok(registry.workflow_step_set_state(
                &req.workflow_run_id,
                &req.step_id,
                req.state,
                req.reason.as_deref(),
            )?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
