use super::*;

#[derive(Clone)]
pub(super) struct ResolvedChatScope {
    pub(super) project_id: Option<Uuid>,
    pub(super) task_id: Option<Uuid>,
    pub(super) flow_id: Option<Uuid>,
    pub(super) project_summary: Option<String>,
    pub(super) task_summary: Option<String>,
    pub(super) flow_summary: Option<String>,
    pub(super) runtime: Option<ProjectRuntimeConfig>,
    pub(super) selection_source: Option<RuntimeSelectionSource>,
}

pub(super) fn resolve_chat_scope(
    service: &ChatService,
    project_ref: Option<&str>,
    task_ref: Option<&str>,
    flow_ref: Option<&str>,
    origin: &'static str,
) -> Result<ResolvedChatScope> {
    let state = service.state()?;
    let explicit_project = match project_ref {
        Some(value) => Some(service.get_project(value)?),
        None => None,
    };
    let task = match task_ref {
        Some(value) => Some(service.get_task(value)?),
        None => None,
    };
    let flow = match flow_ref {
        Some(value) => Some(service.get_flow(value)?),
        None => None,
    };
    let mut resolved_project_id = explicit_project.as_ref().map(|project| project.id);
    if let Some(task) = &task {
        ensure_same_project(&mut resolved_project_id, task.project_id, origin)?;
    }
    if let Some(flow) = &flow {
        ensure_same_project(&mut resolved_project_id, flow.project_id, origin)?;
    }
    let project = match (explicit_project, resolved_project_id) {
        (Some(project), _) => Some(project),
        (None, Some(project_id)) => state.projects.get(&project_id).cloned(),
        (None, None) => None,
    };
    let default_worker_runtime = state.global_runtime_defaults.worker;
    let runtime = if let Some(project) = &project {
        ChatService::project_runtime_for_role_with_source(project, RuntimeRole::Worker).or_else(
            || {
                default_worker_runtime
                    .clone()
                    .map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault))
            },
        )
    } else {
        default_worker_runtime.map(|runtime| (runtime, RuntimeSelectionSource::GlobalDefault))
    };
    let (runtime, selection_source) = runtime.map_or((None, None), |(runtime, source)| {
        (Some(runtime), Some(source))
    });
    Ok(ResolvedChatScope {
        project_id: project.as_ref().map(|value| value.id),
        task_id: task.as_ref().map(|value| value.id),
        flow_id: flow.as_ref().map(|value| value.id),
        project_summary: project.as_ref().map(summarize_project),
        task_summary: task.as_ref().map(summarize_task),
        flow_summary: flow.as_ref().map(summarize_flow),
        runtime,
        selection_source,
    })
}

pub(super) fn resolve_chat_scope_from_session(
    service: &ChatService,
    session: &ChatSessionState,
    origin: &'static str,
) -> Result<ResolvedChatScope> {
    resolve_chat_scope(
        service,
        session.project_id.map(|value| value.to_string()).as_deref(),
        session.task_id.map(|value| value.to_string()).as_deref(),
        session.flow_id.map(|value| value.to_string()).as_deref(),
        origin,
    )
}

pub(super) fn get_chat_session(
    service: &ChatService,
    session_id: &str,
    origin: &'static str,
) -> Result<ChatSessionState> {
    let id = Uuid::parse_str(session_id).map_err(|_| {
        HivemindError::user(
            "invalid_chat_session_id",
            format!("'{session_id}' is not a valid chat session ID"),
            origin,
        )
    })?;
    let state = service.state()?;
    state.chat_sessions.get(&id).cloned().ok_or_else(|| {
        HivemindError::user(
            "chat_session_not_found",
            format!("Chat session '{session_id}' not found"),
            origin,
        )
    })
}

pub(super) fn session_history_inputs(session: &ChatSessionState) -> Vec<ChatHistoryMessageInput> {
    session
        .messages
        .iter()
        .map(|message| ChatHistoryMessageInput {
            role: if message.role == "assistant" {
                ChatHistoryRole::Assistant
            } else {
                ChatHistoryRole::User
            },
            content: message.content.clone(),
        })
        .collect()
}

pub(super) fn default_session_title(mode: ChatMode) -> String {
    format!(
        "{} chat {}",
        mode.as_str(),
        Utc::now().format("%Y-%m-%d %H:%M")
    )
}

pub(super) fn parse_mode(raw: &str) -> ChatMode {
    if raw.eq_ignore_ascii_case("freeflow") {
        ChatMode::Freeflow
    } else {
        ChatMode::Plan
    }
}

pub(super) fn chat_correlation(scope: &ResolvedChatScope) -> CorrelationIds {
    match (scope.project_id, scope.flow_id, scope.task_id) {
        (Some(project_id), Some(flow_id), Some(task_id)) => {
            CorrelationIds::for_flow_task(project_id, flow_id, task_id)
        }
        (Some(project_id), Some(flow_id), None) => CorrelationIds::for_flow(project_id, flow_id),
        (Some(project_id), None, Some(task_id)) => CorrelationIds::for_task(project_id, task_id),
        (Some(project_id), None, None) => CorrelationIds::for_project(project_id),
        _ => CorrelationIds::none(),
    }
}

fn ensure_same_project(
    slot: &mut Option<Uuid>,
    candidate: Uuid,
    origin: &'static str,
) -> Result<()> {
    match slot {
        Some(existing) if *existing != candidate => Err(HivemindError::user(
            "chat_scope_project_mismatch",
            "project/task/flow selection must belong to the same project",
            origin,
        )),
        Some(_) => Ok(()),
        None => {
            *slot = Some(candidate);
            Ok(())
        }
    }
}

fn summarize_project(project: &Project) -> String {
    let mut parts = vec![format!("Project: {} ({})", project.name, project.id)];
    if let Some(description) = project
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Description: {description}"));
    }
    if !project.repositories.is_empty() {
        let repos = project
            .repositories
            .iter()
            .map(|repo| {
                let access_mode = match repo.access_mode {
                    RepoAccessMode::ReadOnly => "read_only",
                    RepoAccessMode::ReadWrite => "read_write",
                };
                format!("{} @ {} [{}]", repo.name, repo.path, access_mode)
            })
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("Repositories: {repos}"));
    }
    parts.join("\n")
}

fn summarize_task(task: &Task) -> String {
    let mut parts = vec![format!("Task: {} ({})", task.title, task.id)];
    parts.push(format!(
        "Task state: {:?}; run mode: {:?}",
        task.state, task.run_mode
    ));
    if let Some(description) = task
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        parts.push(format!("Task description: {description}"));
    }
    parts.join("\n")
}

fn summarize_flow(flow: &TaskFlow) -> String {
    let mut counts = HashMap::<String, usize>::new();
    for execution in flow.task_executions.values() {
        *counts
            .entry(format!("{:?}", execution.state).to_lowercase())
            .or_default() += 1;
    }
    let mut states = counts.into_iter().collect::<Vec<_>>();
    states.sort_by(|a, b| a.0.cmp(&b.0));
    let summary = states
        .into_iter()
        .map(|(state, count)| format!("{state}:{count}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Flow: {}\nFlow state: {:?}; run mode: {:?}\nTask execution states: {}",
        flow.id,
        flow.state,
        flow.run_mode,
        if summary.is_empty() { "none" } else { &summary }
    )
}
