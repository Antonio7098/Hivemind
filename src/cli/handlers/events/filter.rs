use super::*;
use chrono::{DateTime, Utc};
use uuid::Uuid;

fn parse_event_uuid(
    raw: &str,
    code: &str,
    noun: &str,
    origin: &str,
) -> Result<Uuid, crate::core::error::HivemindError> {
    Uuid::parse_str(raw).map_err(|_| {
        crate::core::error::HivemindError::user(
            code,
            format!("'{raw}' is not a valid {noun} ID"),
            origin,
        )
    })
}

fn parse_event_time(
    raw: &str,
    flag: &str,
    origin: &str,
) -> Result<DateTime<Utc>, crate::core::error::HivemindError> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            crate::core::error::HivemindError::user(
                "invalid_timestamp",
                format!("Invalid {flag} timestamp '{raw}'. Expected RFC3339 format."),
                origin,
            )
        })
}

fn parse_non_empty_filter(
    raw: &str,
    code: &str,
    flag: &str,
    origin: &str,
) -> Result<String, crate::core::error::HivemindError> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(crate::core::error::HivemindError::user(
            code,
            format!("{flag} cannot be empty"),
            origin,
        ));
    }
    Ok(normalized.to_string())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_event_filter(
    registry: &Registry,
    origin: &str,
    project: Option<&str>,
    graph: Option<&str>,
    flow: Option<&str>,
    task: Option<&str>,
    attempt: Option<&str>,
    artifact_id: Option<&str>,
    template_id: Option<&str>,
    rule_id: Option<&str>,
    error_type: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    limit: usize,
) -> Result<crate::storage::event_store::EventFilter, crate::core::error::HivemindError> {
    use crate::storage::event_store::EventFilter;

    let mut filter = EventFilter::all();
    filter.limit = Some(limit);

    if let Some(project) = project {
        filter.project_id = Some(registry.get_project(project)?.id);
    }
    if let Some(graph) = graph {
        filter.graph_id = Some(parse_event_uuid(
            graph,
            "invalid_graph_id",
            "graph",
            origin,
        )?);
    }
    if let Some(flow) = flow {
        filter.flow_id = Some(parse_event_uuid(flow, "invalid_flow_id", "flow", origin)?);
    }
    if let Some(task) = task {
        filter.task_id = Some(parse_event_uuid(task, "invalid_task_id", "task", origin)?);
    }
    if let Some(attempt) = attempt {
        filter.attempt_id = Some(parse_event_uuid(
            attempt,
            "invalid_attempt_id",
            "attempt",
            origin,
        )?);
    }
    if let Some(artifact_id) = artifact_id {
        filter.artifact_id = Some(parse_non_empty_filter(
            artifact_id,
            "invalid_artifact_id",
            "--artifact-id",
            origin,
        )?);
    }
    if let Some(template_id) = template_id {
        filter.template_id = Some(parse_non_empty_filter(
            template_id,
            "invalid_template_id",
            "--template-id",
            origin,
        )?);
    }
    if let Some(rule_id) = rule_id {
        filter.rule_id = Some(parse_non_empty_filter(
            rule_id,
            "invalid_rule_id",
            "--rule-id",
            origin,
        )?);
    }
    if let Some(error_type) = error_type {
        filter.error_type = Some(parse_non_empty_filter(
            error_type,
            "invalid_error_type",
            "--error-type",
            origin,
        )?);
    }
    if let Some(since) = since {
        filter.since = Some(parse_event_time(since, "--since", origin)?);
    }
    if let Some(until) = until {
        filter.until = Some(parse_event_time(until, "--until", origin)?);
    }

    if filter
        .since
        .zip(filter.until)
        .is_some_and(|(since, until)| since > until)
    {
        return Err(crate::core::error::HivemindError::user(
            "invalid_time_range",
            "`--since` must be earlier than or equal to `--until`",
            origin,
        ));
    }

    Ok(filter)
}
