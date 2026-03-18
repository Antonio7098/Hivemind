use super::*;

pub(super) fn handle_attempt_queries(
    path: &str,
    url: &str,
    app: &AppContext,
) -> Result<Option<ApiResponse>> {
    let attempt_service = || app.attempt_service();

    let resp = match path {
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
            let attempt = attempt_service()?.get_attempt(attempt_id)?;
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
            let attempt = attempt_service()?.get_attempt(attempt_id)?;
            let diff = if include_diff {
                attempt_service()?.get_attempt_diff(attempt_id)?
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
                "diff": attempt_service()?.get_attempt_diff(attempt_id)?,
            }))?
        }
        _ => return Ok(None),
    };

    Ok(Some(resp))
}
