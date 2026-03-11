#![allow(clippy::too_many_lines)]

use super::*;
use std::str::FromStr;

fn parse_runtime_stream_detail(
    query: &std::collections::HashMap<String, String>,
) -> Result<crate::core::registry::shared_types::RuntimeStreamDetailLevel> {
    let Some(raw) = query.get("detail") else {
        return Ok(crate::core::registry::shared_types::RuntimeStreamDetailLevel::Telemetry);
    };
    crate::core::registry::shared_types::RuntimeStreamDetailLevel::from_str(raw).map_err(|_| {
        HivemindError::user(
            "invalid_runtime_stream_detail",
            format!(
                "Unsupported runtime stream detail '{raw}'. Expected summary, observability, or telemetry"
            ),
            "server/runtime-stream",
        )
    })
}

pub(super) fn handle_get(
    path: &str,
    url: &str,
    default_events_limit: usize,
    registry: &Registry,
) -> Result<Option<ApiResponse>> {
    let resp = match path {
        "/health" => {
            let mut resp = ApiResponse::text(200, "text/plain", "ok\n");
            resp.extra_headers.extend(cors_headers());
            resp
        }
        "/api/version" => {
            super::json_ok(serde_json::json!({"version": env!("CARGO_PKG_VERSION")}))?
        }
        "/api/catalog" => super::json_ok(api_catalog())?,
        "/api/state" => {
            let query = parse_query(url);
            let events_limit = query
                .get("events_limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(build_ui_state(registry, events_limit)?)?
        }
        "/api/projects" => super::json_ok(registry.list_projects()?)?,
        "/api/tasks" => super::json_ok(list_tasks(registry)?)?,
        "/api/graphs" => super::json_ok(list_graphs(registry)?)?,
        "/api/flows" => super::json_ok(list_flows(registry)?)?,
        "/api/merges" => super::json_ok(list_merge_states(registry)?)?,
        "/api/runtimes" => super::json_ok(registry.runtime_list())?,
        "/api/runtimes/health" => {
            let query = parse_query(url);
            let role = parse_runtime_role(
                query.get("role").map(String::as_str),
                "server:runtimes:health",
            )?;
            super::json_ok(registry.runtime_health_with_role(
                query.get("project").map(String::as_str),
                query.get("task").map(String::as_str),
                query.get("flow").map(String::as_str),
                role,
            )?)?
        }
        "/api/events" => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(list_ui_events(registry, limit)?)?
        }
        "/api/events/inspect" => {
            let query = parse_query(url);
            let event_id = query.get("event_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_event_id",
                    "Query parameter 'event_id' is required",
                    "server:events:inspect",
                )
            })?;
            super::json_ok(registry.get_event(event_id)?)?
        }
        "/api/runtime-stream" => {
            let query = parse_query(url);
            let limit = query
                .get("limit")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(default_events_limit);
            super::json_ok(registry.runtime_stream_items_with_detail(
                query.get("flow_id").map(String::as_str),
                query.get("attempt_id").map(String::as_str),
                limit,
                parse_runtime_stream_detail(&query)?,
            )?)?
        }
        "/api/verify/results" => {
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
            super::json_ok(VerifyResultsView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                check_results,
            })?
        }
        "/api/attempts/inspect" => {
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
            super::json_ok(AttemptInspectView {
                attempt_id: attempt.id.to_string(),
                task_id: attempt.task_id.to_string(),
                flow_id: attempt.flow_id.to_string(),
                attempt_number: attempt.attempt_number,
                started_at: attempt.started_at,
                baseline_id: attempt.baseline_id.map(|v| v.to_string()),
                diff_id: attempt.diff_id.map(|v| v.to_string()),
                runtime_session: attempt.runtime_session.as_ref().map(|session| {
                    AttemptRuntimeSessionView {
                        adapter_name: session.adapter_name.clone(),
                        session_id: session.session_id.clone(),
                        discovered_at: session.discovered_at,
                    }
                }),
                turn_refs: attempt
                    .turn_refs
                    .iter()
                    .map(|turn| AttemptTurnRefView {
                        ordinal: turn.ordinal,
                        adapter_name: turn.adapter_name.clone(),
                        stream: format!("{:?}", turn.stream).to_lowercase(),
                        provider_session_id: turn.provider_session_id.clone(),
                        provider_turn_id: turn.provider_turn_id.clone(),
                        git_ref: turn.git_ref.clone(),
                        commit_sha: turn.commit_sha.clone(),
                        summary: turn.summary.clone(),
                    })
                    .collect(),
                diff,
            })?
        }
        "/api/attempts/diff" => {
            let query = parse_query(url);
            let attempt_id = query.get("attempt_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_attempt_id",
                    "Query parameter 'attempt_id' is required",
                    "server:attempts:diff",
                )
            })?;
            super::json_ok(serde_json::json!({
                "attempt_id": attempt_id,
                "diff": registry.get_attempt_diff(attempt_id)?,
            }))?
        }
        "/api/flows/replay" => {
            let query = parse_query(url);
            let flow_id = query.get("flow_id").ok_or_else(|| {
                HivemindError::user(
                    "missing_flow_id",
                    "Query parameter 'flow_id' is required",
                    "server:flows:replay",
                )
            })?;
            super::json_ok(registry.replay_flow(flow_id)?)?
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
            super::json_ok(registry.worktree_list(flow_id)?)?
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
            super::json_ok(registry.worktree_inspect(task_id)?)?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
