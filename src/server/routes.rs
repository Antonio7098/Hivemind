use super::*;

pub(super) fn handle_api_request_inner(
    method: ApiMethod,
    url: &str,
    default_events_limit: usize,
    body: Option<&[u8]>,
    registry: &Registry,
) -> Result<ApiResponse> {
    if method == ApiMethod::Options {
        let mut resp = ApiResponse::text(204, "text/plain", "");
        resp.extra_headers.extend(cors_headers());
        return Ok(resp);
    }

    let (path, _qs) = url.split_once('?').unwrap_or((url, ""));

    match path {
        "/health" if method == ApiMethod::Get => {
            let mut resp = ApiResponse::text(200, "text/plain", "ok\n");
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/version" if method == ApiMethod::Get => {
            let value = serde_json::json!({"version": env!("CARGO_PKG_VERSION")});
            let wrapped = CliResponse::success(value);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/catalog" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(api_catalog());
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/state" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let events_limit = query
                .get("events_limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);

            let state = build_ui_state(registry, events_limit)?;
            let wrapped = CliResponse::success(state);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.list_projects()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_tasks(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_graphs(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_flows(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merges" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(list_merge_states(registry)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtimes" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.runtime_list());
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtimes/health" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let role = parse_runtime_role(
                query.get("role").map(String::as_str),
                "server:runtimes:health",
            )?;
            let wrapped = CliResponse::success(registry.runtime_health_with_role(
                query.get("project").map(String::as_str),
                query.get("task").map(String::as_str),
                query.get("flow").map(String::as_str),
                role,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/events" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            let wrapped = CliResponse::success(list_ui_events(registry, limit)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/events/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let event_id = query.get("event_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_event_id",
                    "Query parameter 'event_id' is required",
                    "server:events:inspect",
                )
            })?;
            let wrapped = CliResponse::success(registry.get_event(event_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/results" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:verify:results",
                )
            })?;
            let output = query.get("output").is_some_and(|v| v == "true");
            let attempt = registry.get_attempt(attempt_id)?;
            let check_results = attempt
                .check_results
                .iter()
                .map(|r| {
                    if output {
                        serde_json::json!(r)
                    } else {
                        serde_json::json!({
                            "name": r.name,
                            "passed": r.passed,
                            "exit_code": r.exit_code,
                            "duration_ms": r.duration_ms,
                            "required": r.required,
                        })
                    }
                })
                .collect::<Vec<_>>();
            let view = VerifyResultsView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                check_results,
            };
            let wrapped = CliResponse::success(view);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/attempts/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:attempts:inspect",
                )
            })?;
            let include_diff = query.get("diff").is_some_and(|v| v == "true");
            let attempt = registry.get_attempt(attempt_id)?;
            let diff = if include_diff {
                registry.get_attempt_diff(attempt_id)?
            } else {
                None
            };
            let view = AttemptInspectView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                started_at: attempt.started_at,
                baseline_id: attempt.baseline_id.map(|v| v.to_string()),
                diff_id: attempt.diff_id.map(|v| v.to_string()),
                diff,
            };
            let wrapped = CliResponse::success(view);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/attempts/diff" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:attempts:diff",
                )
            })?;
            let wrapped = CliResponse::success(serde_json::json!({
                "attempt_id": attempt_id,
                "diff": registry.get_attempt_diff(attempt_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/replay" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:flows:replay",
                )
            })?;
            let wrapped = CliResponse::success(registry.replay_flow(flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:worktrees:list",
                )
            })?;
            let wrapped = CliResponse::success(registry.worktree_list(flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let task_id = query.get("task_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_task_id",
                    "Query parameter 'task_id' is required",
                    "server:worktrees:inspect",
                )
            })?;
            let wrapped = CliResponse::success(registry.worktree_inspect(task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/create" if method == ApiMethod::Post => {
            let req: ProjectCreateRequest = parse_json_body(body, "server:projects:create")?;
            let wrapped = CliResponse::success(
                registry.create_project(&req.name, req.description.as_deref())?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/update" if method == ApiMethod::Post => {
            let req: ProjectUpdateRequest = parse_json_body(body, "server:projects:update")?;
            let wrapped = CliResponse::success(registry.update_project(
                &req.project,
                req.name.as_deref(),
                req.description.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/delete" if method == ApiMethod::Post => {
            let req: ProjectDeleteRequest = parse_json_body(body, "server:projects:delete")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "project_id": registry.delete_project(&req.project)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/runtime" if method == ApiMethod::Post => {
            let req: ProjectRuntimeRequest = parse_json_body(body, "server:projects:runtime")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:projects:runtime")?;
            let wrapped = CliResponse::success(registry.project_runtime_set_role(
                &req.project,
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/runtime/defaults" if method == ApiMethod::Post => {
            let req: RuntimeDefaultsSetRequest =
                parse_json_body(body, "server:runtime:defaults:set")?;
            let env_pairs = env_pairs_from_map(req.env);
            let role = parse_runtime_role(req.role.as_deref(), "server:runtime:defaults:set")?;
            registry.runtime_defaults_set(
                role,
                req.adapter.as_deref().unwrap_or("opencode"),
                req.binary_path.as_deref().unwrap_or("opencode"),
                req.model,
                &req.args.unwrap_or_default(),
                &env_pairs,
                req.timeout_ms.unwrap_or(600_000),
                req.max_parallel_tasks.unwrap_or(1),
            )?;
            let wrapped = CliResponse::success(serde_json::json!({ "ok": true }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/repos/attach" if method == ApiMethod::Post => {
            let req: ProjectAttachRepoRequest =
                parse_json_body(body, "server:projects:repos:attach")?;
            let access_mode = match req
                .access
                .as_deref()
                .unwrap_or("rw")
                .to_lowercase()
                .as_str()
            {
                "ro" | "readonly" => crate::core::scope::RepoAccessMode::ReadOnly,
                "rw" | "readwrite" => crate::core::scope::RepoAccessMode::ReadWrite,
                other => {
                    return Err(HivemindError::user(
                        "invalid_access_mode",
                        format!("Invalid access mode '{other}'"),
                        "server:projects:repos:attach",
                    ));
                }
            };
            let wrapped = CliResponse::success(registry.attach_repo(
                &req.project,
                &req.path,
                req.name.as_deref(),
                access_mode,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/projects/repos/detach" if method == ApiMethod::Post => {
            let req: ProjectDetachRepoRequest =
                parse_json_body(body, "server:projects:repos:detach")?;
            let wrapped = CliResponse::success(registry.detach_repo(&req.project, &req.repo_name)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/create" if method == ApiMethod::Post => {
            let req: TaskCreateRequest = parse_json_body(body, "server:tasks:create")?;
            let wrapped = CliResponse::success(registry.create_task(
                &req.project,
                &req.title,
                req.description.as_deref(),
                req.scope,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/update" if method == ApiMethod::Post => {
            let req: TaskUpdateRequest = parse_json_body(body, "server:tasks:update")?;
            let wrapped = CliResponse::success(registry.update_task(
                &req.task_id,
                req.title.as_deref(),
                req.description.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/delete" if method == ApiMethod::Post => {
            let req: TaskDeleteRequest = parse_json_body(body, "server:tasks:delete")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "task_id": registry.delete_task(&req.task_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/close" if method == ApiMethod::Post => {
            let req: TaskCloseRequest = parse_json_body(body, "server:tasks:close")?;
            let wrapped =
                CliResponse::success(registry.close_task(&req.task_id, req.reason.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/start" if method == ApiMethod::Post => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:start")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "attempt_id": registry.start_task_execution(&req.task_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/complete" if method == ApiMethod::Post => {
            let req: TaskIdRequest = parse_json_body(body, "server:tasks:complete")?;
            let wrapped = CliResponse::success(registry.complete_task_execution(&req.task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/retry" if method == ApiMethod::Post => {
            let req: TaskRetryRequest = parse_json_body(body, "server:tasks:retry")?;
            let mode = match req
                .mode
                .as_deref()
                .unwrap_or("clean")
                .to_lowercase()
                .as_str()
            {
                "clean" => RetryMode::Clean,
                "continue" => RetryMode::Continue,
                other => {
                    return Err(HivemindError::user(
                        "invalid_retry_mode",
                        format!("Invalid retry mode '{other}'"),
                        "server:tasks:retry",
                    ));
                }
            };
            let wrapped = CliResponse::success(registry.retry_task(
                &req.task_id,
                req.reset_count.unwrap_or(false),
                mode,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/abort" if method == ApiMethod::Post => {
            let req: TaskAbortRequest = parse_json_body(body, "server:tasks:abort")?;
            let wrapped =
                CliResponse::success(registry.abort_task(&req.task_id, req.reason.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/runtime" if method == ApiMethod::Post => {
            let req: TaskRuntimeSetRequest = parse_json_body(body, "server:tasks:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:tasks:runtime")?;
            let wrapped = if req.clear.unwrap_or(false) {
                CliResponse::success(registry.task_runtime_clear_role(&req.task_id, role)?)
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                CliResponse::success(registry.task_runtime_set_role(
                    &req.task_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                )?)
            };
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/tasks/run-mode" if method == ApiMethod::Post => {
            let req: TaskSetRunModeRequest = parse_json_body(body, "server:tasks:run-mode")?;
            let wrapped = CliResponse::success(registry.task_set_run_mode(
                &req.task_id,
                parse_run_mode(&req.mode, "server:tasks:run-mode")?,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/create" if method == ApiMethod::Post => {
            let req: GraphCreateRequest = parse_json_body(body, "server:graphs:create")?;
            let wrapped = CliResponse::success(
                registry.create_graph(
                    &req.project,
                    &req.name,
                    &req.from_tasks
                        .iter()
                        .map(|s| {
                            uuid::Uuid::parse_str(s).map_err(|_| {
                                HivemindError::user(
                                    "invalid_task_id",
                                    format!("'{s}' is not a valid task ID"),
                                    "server:graphs:create",
                                )
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                )?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/delete" if method == ApiMethod::Post => {
            let req: GraphDeleteRequest = parse_json_body(body, "server:graphs:delete")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "graph_id": registry.delete_graph(&req.graph_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/dependencies/add" if method == ApiMethod::Post => {
            let req: GraphDependencyRequest =
                parse_json_body(body, "server:graphs:dependencies:add")?;
            let wrapped = CliResponse::success(registry.add_graph_dependency(
                &req.graph_id,
                &req.from_task,
                &req.to_task,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/checks/add" if method == ApiMethod::Post => {
            let req: GraphAddCheckRequest = parse_json_body(body, "server:graphs:checks:add")?;
            let mut check = CheckConfig::new(req.name, req.command);
            check.required = req.required.unwrap_or(true);
            check.timeout_ms = req.timeout_ms;
            let wrapped = CliResponse::success(registry.add_graph_task_check(
                &req.graph_id,
                &req.task_id,
                check,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/graphs/validate" if method == ApiMethod::Post => {
            let req: GraphValidateRequest = parse_json_body(body, "server:graphs:validate")?;
            let wrapped = CliResponse::success(registry.validate_graph(&req.graph_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/create" if method == ApiMethod::Post => {
            let req: FlowCreateRequest = parse_json_body(body, "server:flows:create")?;
            let wrapped =
                CliResponse::success(registry.create_flow(&req.graph_id, req.name.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/delete" if method == ApiMethod::Post => {
            let req: FlowDeleteRequest = parse_json_body(body, "server:flows:delete")?;
            let wrapped = CliResponse::success(serde_json::json!({
                "flow_id": registry.delete_flow(&req.flow_id)?,
            }));
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/start" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:start")?;
            let wrapped = CliResponse::success(registry.start_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/tick" if method == ApiMethod::Post => {
            let req: FlowTickRequest = parse_json_body(body, "server:flows:tick")?;
            let wrapped = CliResponse::success(registry.tick_flow(
                &req.flow_id,
                req.interactive.unwrap_or(false),
                req.max_parallel,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/pause" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:pause")?;
            let wrapped = CliResponse::success(registry.pause_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/resume" if method == ApiMethod::Post => {
            let req: FlowIdRequest = parse_json_body(body, "server:flows:resume")?;
            let wrapped = CliResponse::success(registry.resume_flow(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/abort" if method == ApiMethod::Post => {
            let req: FlowAbortRequest = parse_json_body(body, "server:flows:abort")?;
            let wrapped = CliResponse::success(registry.abort_flow(
                &req.flow_id,
                req.reason.as_deref(),
                req.force.unwrap_or(false),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/run-mode" if method == ApiMethod::Post => {
            let req: FlowSetRunModeRequest = parse_json_body(body, "server:flows:run-mode")?;
            let wrapped = CliResponse::success(registry.flow_set_run_mode(
                &req.flow_id,
                parse_run_mode(&req.mode, "server:flows:run-mode")?,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/dependencies/add" if method == ApiMethod::Post => {
            let req: FlowAddDependencyRequest =
                parse_json_body(body, "server:flows:dependencies:add")?;
            let wrapped = CliResponse::success(
                registry.flow_add_dependency(&req.flow_id, &req.depends_on_flow_id)?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/flows/runtime" if method == ApiMethod::Post => {
            let req: FlowRuntimeSetRequest = parse_json_body(body, "server:flows:runtime")?;
            let role = parse_runtime_role(req.role.as_deref(), "server:flows:runtime")?;
            let wrapped = if req.clear.unwrap_or(false) {
                CliResponse::success(registry.flow_runtime_clear(&req.flow_id, role)?)
            } else {
                let env_pairs = env_pairs_from_map(req.env);
                CliResponse::success(registry.flow_runtime_set(
                    &req.flow_id,
                    role,
                    req.adapter.as_deref().unwrap_or("opencode"),
                    req.binary_path.as_deref().unwrap_or("opencode"),
                    req.model,
                    &req.args.unwrap_or_default(),
                    &env_pairs,
                    req.timeout_ms.unwrap_or(600_000),
                    req.max_parallel_tasks.unwrap_or(1),
                )?)
            };
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/override" if method == ApiMethod::Post => {
            let req: VerifyOverrideRequest = parse_json_body(body, "server:verify:override")?;
            let wrapped = CliResponse::success(registry.verify_override(
                &req.task_id,
                &req.decision,
                &req.reason,
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/verify/run" if method == ApiMethod::Post => {
            let req: VerifyRunRequest = parse_json_body(body, "server:verify:run")?;
            let wrapped = CliResponse::success(registry.verify_run(&req.task_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/prepare" if method == ApiMethod::Post => {
            let req: MergePrepareRequest = parse_json_body(body, "server:merge:prepare")?;
            let wrapped =
                CliResponse::success(registry.merge_prepare(&req.flow_id, req.target.as_deref())?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/approve" if method == ApiMethod::Post => {
            let req: MergeApproveRequest = parse_json_body(body, "server:merge:approve")?;
            let wrapped = CliResponse::success(registry.merge_approve(&req.flow_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/merge/execute" if method == ApiMethod::Post => {
            let req: MergeExecuteRequest = parse_json_body(body, "server:merge:execute")?;
            let wrapped = CliResponse::success(registry.merge_execute_with_options(
                &req.flow_id,
                MergeExecuteOptions {
                    mode: parse_merge_mode(req.mode.as_deref(), "server:merge:execute")?,
                    monitor_ci: req.monitor_ci.unwrap_or(false),
                    auto_merge: req.auto_merge.unwrap_or(false),
                    pull_after: req.pull_after.unwrap_or(false),
                },
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/checkpoints/complete" if method == ApiMethod::Post => {
            let req: CheckpointCompleteRequest =
                parse_json_body(body, "server:checkpoints:complete")?;
            let wrapped = CliResponse::success(registry.checkpoint_complete(
                &req.attempt_id,
                &req.checkpoint_id,
                req.summary.as_deref(),
            )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/worktrees/cleanup" if method == ApiMethod::Post => {
            let req: WorktreeCleanupRequest = parse_json_body(body, "server:worktrees:cleanup")?;
            let result = registry.worktree_cleanup(&req.flow_id, req.force, req.dry_run)?;
            let wrapped = CliResponse::success(result);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/constitution" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(registry.constitution_show(project)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/constitution/check" if method == ApiMethod::Post => {
            let req: ProjectIdRequest =
                parse_json_body(body, "server:governance:constitution:check")?;
            let wrapped = CliResponse::success(registry.constitution_check(&req.project)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/documents" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(registry.project_governance_document_list(project)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/documents/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            let document_id = query.get("document_id").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(
                registry.project_governance_document_inspect(project, document_id)?,
            );
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/notepad" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let project = query.get("project").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(registry.project_governance_notepad_show(project)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/global/notepad" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.global_notepad_show()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/global/skills" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.global_skill_list()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/global/skills/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let skill_id = query.get("skill_id").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(registry.global_skill_inspect(skill_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/global/templates" if method == ApiMethod::Get => {
            let wrapped = CliResponse::success(registry.global_template_list()?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/global/templates/inspect" if method == ApiMethod::Get => {
            let query = parse_query(url);
            let template_id = query.get("template_id").map_or("", |s| s.as_str());
            let wrapped = CliResponse::success(registry.global_template_inspect(template_id)?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/api/governance/graph-snapshot/refresh" if method == ApiMethod::Post => {
            let req: GraphSnapshotRefreshRequest =
                parse_json_body(body, "server:governance:graph-snapshot:refresh")?;
            let wrapped =
                CliResponse::success(registry.graph_snapshot_refresh(
                    &req.project,
                    req.trigger.as_deref().unwrap_or("api"),
                )?);
            let mut resp = ApiResponse::json(200, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
        "/health"
        | "/api/version"
        | "/api/catalog"
        | "/api/state"
        | "/api/projects"
        | "/api/tasks"
        | "/api/graphs"
        | "/api/flows"
        | "/api/merges"
        | "/api/runtimes"
        | "/api/runtimes/health"
        | "/api/events"
        | "/api/events/inspect"
        | "/api/verify/results"
        | "/api/attempts/inspect"
        | "/api/attempts/diff"
        | "/api/flows/replay"
        | "/api/worktrees"
        | "/api/worktrees/inspect"
        | "/api/governance/constitution"
        | "/api/governance/documents"
        | "/api/governance/documents/inspect"
        | "/api/governance/notepad"
        | "/api/governance/global/notepad"
        | "/api/governance/global/skills"
        | "/api/governance/global/skills/inspect"
        | "/api/governance/global/templates"
        | "/api/governance/global/templates/inspect" => method_not_allowed(path, method, "GET"),
        "/api/projects/create"
        | "/api/projects/update"
        | "/api/projects/delete"
        | "/api/projects/runtime"
        | "/api/runtime/defaults"
        | "/api/projects/repos/attach"
        | "/api/projects/repos/detach"
        | "/api/tasks/create"
        | "/api/tasks/update"
        | "/api/tasks/delete"
        | "/api/tasks/runtime"
        | "/api/tasks/run-mode"
        | "/api/tasks/close"
        | "/api/tasks/start"
        | "/api/tasks/complete"
        | "/api/tasks/retry"
        | "/api/tasks/abort"
        | "/api/graphs/create"
        | "/api/graphs/delete"
        | "/api/graphs/dependencies/add"
        | "/api/graphs/checks/add"
        | "/api/graphs/validate"
        | "/api/flows/create"
        | "/api/flows/delete"
        | "/api/flows/start"
        | "/api/flows/tick"
        | "/api/flows/pause"
        | "/api/flows/resume"
        | "/api/flows/abort"
        | "/api/flows/run-mode"
        | "/api/flows/dependencies/add"
        | "/api/flows/runtime"
        | "/api/verify/override"
        | "/api/verify/run"
        | "/api/merge/prepare"
        | "/api/merge/approve"
        | "/api/merge/execute"
        | "/api/checkpoints/complete"
        | "/api/worktrees/cleanup"
        | "/api/governance/constitution/check"
        | "/api/governance/graph-snapshot/refresh" => method_not_allowed(path, method, "POST"),
        _ => {
            let err = HivemindError::user(
                "endpoint_not_found",
                format!("Unknown endpoint '{path}'"),
                "server:handle_api_request",
            );
            let wrapped = CliResponse::<()>::error(&err);
            let mut resp = ApiResponse::json(404, &wrapped)?;
            resp.extra_headers.extend(cors_headers());
            Ok(resp)
        }
    }
}
