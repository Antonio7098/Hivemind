use super::turn_items::{TurnItem, TurnItemKind, TurnItemOutcome};
use crate::core::graph_query::{GraphQueryEdge, GRAPH_QUERY_ENV_PROJECT_ID};
use crate::native::runtime_hardening::{NativeRuntimeStateStore, RuntimeHardeningConfig};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

const LEASE_TOOL_NAME: &str = "retain_context";
const MAX_LEASE_TURNS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ContextRepresentationLevel {
    Hidden,
    HandleOnly,
    GraphCard,
    ExactExcerpt,
    ExpandedExcerpt,
}

impl ContextRepresentationLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::HandleOnly => "handle_only",
            Self::GraphCard => "graph_card",
            Self::ExactExcerpt => "exact_excerpt",
            Self::ExpandedExcerpt => "expanded_excerpt",
        }
    }

    pub(crate) fn is_excerpt(self) -> bool {
        matches!(self, Self::ExactExcerpt | Self::ExpandedExcerpt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GraphWorkingSetHints {
    pub(crate) current_focus_paths: HashSet<String>,
    pub(crate) pinned_paths: HashSet<String>,
    pub(crate) working_set_paths: HashSet<String>,
    pub(crate) hydrated_paths: HashSet<String>,
    pub(crate) recent_traversal_paths: HashSet<String>,
    pub(crate) adjacent_paths: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextWindowSignals {
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) freshness: String,
    pub(crate) last_turn_index: Option<u32>,
    pub(crate) changed_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextVisibilityDecision {
    pub(crate) relevance_score: u32,
    pub(crate) desired_level: ContextRepresentationLevel,
    pub(crate) minimum_floor: ContextRepresentationLevel,
    pub(crate) turns_until_next_downgrade: Option<u32>,
    pub(crate) lease_remaining_turns: Option<u32>,
    pub(crate) graph_neighbor_paths: Vec<String>,
    pub(crate) reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct LeaseRequest {
    path: String,
    turns: u32,
    floor: Option<ContextRepresentationLevel>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionNodeRef {
    node_id: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionTraversal {
    #[serde(default)]
    paths: Vec<String>,
}

pub(crate) fn load_graph_working_set_hints(
    runtime_env: &HashMap<String, String>,
) -> GraphWorkingSetHints {
    let Some(project_id) = runtime_env
        .get(GRAPH_QUERY_ENV_PROJECT_ID)
        .filter(|v| !v.trim().is_empty())
    else {
        return GraphWorkingSetHints::default();
    };
    let store = match NativeRuntimeStateStore::open(&RuntimeHardeningConfig::from_env(runtime_env))
    {
        Ok(store) => store,
        Err(_) => return GraphWorkingSetHints::default(),
    };
    let artifact = match store.graphcode_artifact_by_project(project_id, "codegraph") {
        Ok(Some(artifact)) => artifact,
        _ => return GraphWorkingSetHints::default(),
    };
    let Some(session_ref) = artifact.active_session_ref.as_deref() else {
        return GraphWorkingSetHints::default();
    };
    let session = match store.graphcode_session_by_ref(session_ref) {
        Ok(Some(session)) => session,
        _ => return GraphWorkingSetHints::default(),
    };
    parse_graph_working_set_hints(
        &session.current_focus_json,
        &session.pinned_nodes_json,
        &session.working_set_refs_json,
        &session.hydrated_excerpts_json,
        &session.recent_traversals_json,
        &session.path_artifacts_json,
    )
}

pub(crate) fn score_context_windows(
    task_text: &str,
    current_turn_index: u32,
    history: &[TurnItem],
    windows: &[ContextWindowSignals],
    graph_hints: &GraphWorkingSetHints,
) -> HashMap<String, ContextVisibilityDecision> {
    let recent_action_paths = recent_action_paths(history, current_turn_index);
    let dirty_paths = windows
        .iter()
        .filter(|w| w.status == "dirty")
        .map(|w| w.path.clone())
        .collect::<HashSet<_>>();
    let lease_requests = granted_leases(history, current_turn_index);
    windows
        .iter()
        .map(|window| {
            let decision = score_single_window(
                task_text,
                current_turn_index,
                window,
                &dirty_paths,
                &recent_action_paths,
                graph_hints,
                lease_requests.get(&window.path),
            );
            (window.path.clone(), decision)
        })
        .collect()
}

fn score_single_window(
    task_text: &str,
    current_turn_index: u32,
    window: &ContextWindowSignals,
    dirty_paths: &HashSet<String>,
    recent_action_paths: &HashSet<String>,
    graph_hints: &GraphWorkingSetHints,
    lease: Option<&LeaseRequest>,
) -> ContextVisibilityDecision {
    let mut score = 20i32;
    let mut floor = ContextRepresentationLevel::HandleOnly;
    let mut reasons = Vec::new();
    if task_text.contains(&window.path) {
        score += 30;
        reasons.push("task_path".to_string());
    }
    if window.status == "dirty" {
        score += 40;
        floor = ContextRepresentationLevel::ExactExcerpt;
        reasons.push("dirty".to_string());
    }
    if matches!(
        window.freshness.as_str(),
        "initial_read" | "reread_unchanged" | "reread_changed" | "confirmed_write"
    ) {
        score += 16;
        reasons.push(window.freshness.clone());
    }
    if matches!(
        window.freshness.as_str(),
        "initial_write" | "write_updates_dirty" | "write_supersedes_read"
    ) {
        score += 12;
        reasons.push(window.freshness.clone());
    }
    if recent_action_paths.contains(&window.path) {
        score += 16;
        reasons.push("recent_action".to_string());
    }
    if graph_hints.current_focus_paths.contains(&window.path) {
        score += 12;
        reasons.push("graph_focus".to_string());
    }
    if graph_hints.hydrated_paths.contains(&window.path) {
        score += 12;
        reasons.push("graph_hydrated".to_string());
    }
    if graph_hints.pinned_paths.contains(&window.path) {
        score += 10;
        floor = floor.max(ContextRepresentationLevel::GraphCard);
        reasons.push("graph_pinned".to_string());
    }
    if graph_hints.working_set_paths.contains(&window.path) {
        score += 8;
        reasons.push("graph_working_set".to_string());
    }
    if graph_hints.recent_traversal_paths.contains(&window.path) {
        score += 6;
        reasons.push("graph_recent_traversal".to_string());
    }
    if window.changed_line_count > 0 {
        score += 6;
        reasons.push("changed_lines".to_string());
    }
    let graph_neighbors = graph_hints
        .adjacent_paths
        .get(&window.path)
        .cloned()
        .unwrap_or_default();
    let supporting_dirty = graph_neighbors
        .iter()
        .filter(|path| dirty_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();
    if !supporting_dirty.is_empty() {
        score += 20;
        floor = floor.max(ContextRepresentationLevel::GraphCard);
        reasons.push("graph_supports_dirty".to_string());
    }
    if supporting_dirty.is_empty()
        && dirty_paths
            .iter()
            .any(|path| same_directory(path, &window.path))
    {
        score += 6;
        reasons.push("same_directory_as_dirty".to_string());
    }
    if let Some(last_turn) = window.last_turn_index {
        let untouched_turns = current_turn_index.saturating_sub(last_turn.saturating_add(1));
        if untouched_turns <= 2 {
            score += 8;
            reasons.push("recent_turn".to_string());
        } else if window.status != "dirty" {
            score -= 4 * i32::try_from(untouched_turns.saturating_sub(2)).unwrap_or(i32::MAX);
        }
    }
    let lease_remaining_turns = lease.and_then(|lease| (lease.turns > 0).then_some(lease.turns));
    if let Some(lease) = lease {
        score += i32::try_from((5 + lease.turns / 2).min(MAX_LEASE_TURNS)).unwrap_or(10);
        floor = floor.max(lease.floor.unwrap_or(ContextRepresentationLevel::GraphCard));
        reasons.push("lease_active".to_string());
    }
    let score = score.clamp(0, 100) as u32;
    let mut desired_level = desired_level_for_score(score).max(floor);
    if window.status == "dirty" && desired_level < ContextRepresentationLevel::ExactExcerpt {
        desired_level = ContextRepresentationLevel::ExactExcerpt;
    }
    let turns_until_next_downgrade =
        downgrade_horizon(score, desired_level, floor, lease_remaining_turns);
    ContextVisibilityDecision {
        relevance_score: score,
        desired_level,
        minimum_floor: floor,
        turns_until_next_downgrade,
        lease_remaining_turns,
        graph_neighbor_paths: supporting_dirty,
        reason_codes: reasons,
    }
}

fn desired_level_for_score(score: u32) -> ContextRepresentationLevel {
    match score {
        75..=u32::MAX => ContextRepresentationLevel::ExpandedExcerpt,
        55..=74 => ContextRepresentationLevel::ExactExcerpt,
        30..=54 => ContextRepresentationLevel::GraphCard,
        15..=29 => ContextRepresentationLevel::HandleOnly,
        _ => ContextRepresentationLevel::Hidden,
    }
}

fn downgrade_horizon(
    score: u32,
    current: ContextRepresentationLevel,
    floor: ContextRepresentationLevel,
    lease_remaining_turns: Option<u32>,
) -> Option<u32> {
    let next_threshold = match current {
        ContextRepresentationLevel::ExpandedExcerpt
            if floor < ContextRepresentationLevel::ExpandedExcerpt =>
        {
            Some(74)
        }
        ContextRepresentationLevel::ExactExcerpt
            if floor < ContextRepresentationLevel::ExactExcerpt =>
        {
            Some(54)
        }
        ContextRepresentationLevel::GraphCard if floor < ContextRepresentationLevel::GraphCard => {
            Some(29)
        }
        ContextRepresentationLevel::HandleOnly
            if floor < ContextRepresentationLevel::HandleOnly =>
        {
            Some(14)
        }
        _ => None,
    }?;
    let score_gap = score.saturating_sub(next_threshold);
    let base = if score_gap == 0 {
        1
    } else {
        (score_gap + 3) / 4
    };
    Some(lease_remaining_turns.map_or(base, |lease_turns| base.max(lease_turns)))
}

fn granted_leases(history: &[TurnItem], current_turn_index: u32) -> HashMap<String, LeaseRequest> {
    let requests = history
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolCall {
                call_id,
                tool_name,
                request,
            } if tool_name == LEASE_TOOL_NAME => {
                Some((call_id.clone(), parse_lease_request(request)))
            }
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    history
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolResult {
                call_id,
                tool_name,
                outcome: TurnItemOutcome::Success,
                ..
            } if tool_name == LEASE_TOOL_NAME => {
                let turn_index = item.provenance.turn_index.unwrap_or_default();
                let mut request = requests.get(call_id)?.clone();
                let expires_after_turn = turn_index.saturating_add(request.turns.saturating_sub(1));
                if current_turn_index > expires_after_turn.saturating_add(1) {
                    return None;
                }
                request.turns = expires_after_turn
                    .saturating_sub(current_turn_index)
                    .saturating_add(1);
                Some((request.path.clone(), request))
            }
            _ => None,
        })
        .collect()
}

fn parse_lease_request(request: &str) -> LeaseRequest {
    let value = serde_json::from_str::<serde_json::Value>(request).unwrap_or_default();
    let input = value.get("input").unwrap_or(&value);
    let path = input
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let turns = input
        .get("turns")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(3)
        .clamp(1, u64::from(MAX_LEASE_TURNS)) as u32;
    let floor = input
        .get("floor")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_floor);
    LeaseRequest { path, turns, floor }
}

fn parse_floor(value: &str) -> Option<ContextRepresentationLevel> {
    match value {
        "handle_only" => Some(ContextRepresentationLevel::HandleOnly),
        "graph_card" => Some(ContextRepresentationLevel::GraphCard),
        "exact_excerpt" => Some(ContextRepresentationLevel::ExactExcerpt),
        "expanded_excerpt" => Some(ContextRepresentationLevel::ExpandedExcerpt),
        _ => None,
    }
}

fn recent_action_paths(history: &[TurnItem], current_turn_index: u32) -> HashSet<String> {
    history
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::AssistantText { directive, content } if directive == "act" => {
                item.provenance.turn_index.and_then(|turn| {
                    (current_turn_index.saturating_sub(turn) <= 3)
                        .then(|| assistant_action_path(content))?
                })
            }
            _ => None,
        })
        .collect()
}

fn assistant_action_path(action: &str) -> Option<String> {
    let mut parts = action.splitn(3, ':');
    if parts.next()? != "tool" {
        return None;
    }
    let _tool_name = parts.next()?;
    serde_json::from_str::<serde_json::Value>(parts.next().unwrap_or_default())
        .ok()?
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
}

fn same_directory(left: &str, right: &str) -> bool {
    left.rsplit_once('/').map(|(dir, _)| dir) == right.rsplit_once('/').map(|(dir, _)| dir)
}

fn parse_graph_working_set_hints(
    current_focus_json: &str,
    pinned_nodes_json: &str,
    working_set_refs_json: &str,
    hydrated_excerpts_json: &str,
    recent_traversals_json: &str,
    path_artifacts_json: &str,
) -> GraphWorkingSetHints {
    let pinned = serde_json::from_str::<Vec<SessionNodeRef>>(pinned_nodes_json).unwrap_or_default();
    let working =
        serde_json::from_str::<Vec<SessionNodeRef>>(working_set_refs_json).unwrap_or_default();
    let hydrated =
        serde_json::from_str::<Vec<SessionNodeRef>>(hydrated_excerpts_json).unwrap_or_default();
    let traversals =
        serde_json::from_str::<Vec<SessionTraversal>>(recent_traversals_json).unwrap_or_default();
    let current_focus_ids =
        serde_json::from_str::<Vec<String>>(current_focus_json).unwrap_or_default();
    let edges =
        serde_json::from_str::<Vec<GraphQueryEdge>>(path_artifacts_json).unwrap_or_default();
    let node_path_by_id = pinned
        .iter()
        .chain(working.iter())
        .chain(hydrated.iter())
        .filter_map(|node| {
            node.path
                .as_ref()
                .map(|path| (node.node_id.clone(), path.clone()))
        })
        .collect::<HashMap<_, _>>();
    let current_focus_paths = current_focus_ids
        .iter()
        .filter_map(|id| node_path_by_id.get(id).cloned())
        .collect::<HashSet<_>>();
    let pinned_paths = pinned
        .into_iter()
        .filter_map(|node| node.path)
        .collect::<HashSet<_>>();
    let working_set_paths = working
        .into_iter()
        .filter_map(|node| node.path)
        .collect::<HashSet<_>>();
    let hydrated_paths = hydrated
        .into_iter()
        .filter_map(|node| node.path)
        .collect::<HashSet<_>>();
    let recent_traversal_paths = traversals
        .into_iter()
        .flat_map(|traversal| traversal.paths.into_iter())
        .collect::<HashSet<_>>();
    let mut adjacent_paths = HashMap::<String, HashSet<String>>::new();
    for edge in edges {
        let (Some(source_path), Some(target_path)) = (
            node_path_by_id.get(&edge.source),
            node_path_by_id.get(&edge.target),
        ) else {
            continue;
        };
        if source_path == target_path {
            continue;
        }
        adjacent_paths
            .entry(source_path.clone())
            .or_default()
            .insert(target_path.clone());
        adjacent_paths
            .entry(target_path.clone())
            .or_default()
            .insert(source_path.clone());
    }
    GraphWorkingSetHints {
        current_focus_paths,
        pinned_paths,
        working_set_paths,
        hydrated_paths,
        recent_traversal_paths,
        adjacent_paths,
    }
}
