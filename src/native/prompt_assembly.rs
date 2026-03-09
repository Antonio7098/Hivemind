use super::*;
use crate::adapters::runtime::{ExecutionInput, NativePromptMetadata};
use crate::native::tool_engine::ToolContract;
use crate::native::turn_items::{TurnItemKind, TurnItemOutcome};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePromptAssembly {
    pub base_runtime_instructions: String,
    pub mode_contract: String,
    pub mode_contract_hash: String,
    pub objective_state: String,
    #[serde(default)]
    pub selected_history: Vec<NativePromptItem>,
    #[serde(default)]
    pub active_code_windows: Vec<NativePromptItem>,
    #[serde(default)]
    pub code_navigation: Vec<NativePromptItem>,
    #[serde(default)]
    pub tool_contracts: Vec<String>,
    #[serde(default)]
    pub compacted_summaries: Vec<NativePromptItem>,
    pub prompt_headroom: usize,
    pub available_budget: usize,
    pub rendered_prompt_hash: String,
    pub rendered_prompt_bytes: usize,
    #[serde(default)]
    pub runtime_context_bytes: usize,
    #[serde(default)]
    pub static_prompt_chars: usize,
    #[serde(default)]
    pub objective_state_chars: usize,
    #[serde(default)]
    pub selected_history_chars: usize,
    #[serde(default)]
    pub active_code_window_chars: usize,
    #[serde(default)]
    pub compacted_summary_chars: usize,
    #[serde(default)]
    pub code_navigation_chars: usize,
    #[serde(default)]
    pub tool_contract_chars: usize,
    #[serde(default)]
    pub assembly_duration_ms: u64,
    pub selected_item_count: usize,
    #[serde(default)]
    pub selected_history_count: usize,
    #[serde(default)]
    pub active_code_window_count: usize,
    #[serde(default)]
    pub code_navigation_count: usize,
    #[serde(default)]
    pub compacted_summary_count: usize,
    #[serde(default)]
    pub tool_contract_count: usize,
    #[serde(default)]
    pub skipped_item_count: usize,
    #[serde(default)]
    pub truncated_item_count: usize,
    #[serde(default)]
    pub tool_result_items_visible: usize,
    #[serde(default)]
    pub latest_tool_result_turn_index: Option<u32>,
    #[serde(default)]
    pub latest_tool_names_visible: Vec<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,
    #[serde(default)]
    pub inputs_hash: Option<String>,
    #[serde(default)]
    pub delivered_context_hash: Option<String>,
    #[serde(default)]
    pub rendered_context_hash: Option<String>,
    #[serde(default)]
    pub context_window_state_hash: Option<String>,
    #[serde(default)]
    pub delivery_target: Option<String>,
    #[serde(default)]
    pub overflow_classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePromptItem {
    pub item_id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativePromptRender {
    pub(crate) prompt: String,
    pub(crate) assembly: NativePromptAssembly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativePromptSelection {
    items: Vec<TurnItem>,
    skipped_item_count: usize,
    truncated_item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveCodeWindow {
    path: String,
    source_tool: String,
    originating_call_id: String,
    represented_call_ids: Vec<String>,
    status: String,
    freshness: String,
    current_digest: String,
    current_content: String,
    changed_lines: Vec<(usize, usize)>,
    last_turn_index: Option<u32>,
}

impl ActiveCodeWindow {
    fn render_for_prompt(&self) -> String {
        let changed_lines = if self.changed_lines.is_empty() {
            "changed_lines=none".to_string()
        } else {
            format!(
                "changed_lines={}",
                self.changed_lines
                    .iter()
                    .map(|(start, end)| format!("{start}-{end}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        format!(
            "[code_window:{}:{}] path={} call_id={} freshness={} digest={} content_chars={} {}\n{}",
            self.status,
            self.source_tool,
            self.path,
            self.originating_call_id,
            self.freshness,
            short_hash(&self.current_digest),
            self.current_content.chars().count(),
            changed_lines,
            self.current_content,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ActiveCodeWindows {
    windows: Vec<ActiveCodeWindow>,
    represented_call_ids: HashSet<String>,
    represented_paths: HashSet<String>,
}

impl ActiveCodeWindows {
    fn from_windows(windows: Vec<ActiveCodeWindow>) -> Self {
        let represented_call_ids = windows
            .iter()
            .flat_map(|window| window.represented_call_ids.iter().cloned())
            .collect::<HashSet<_>>();
        let represented_paths = windows
            .iter()
            .map(|window| window.path.clone())
            .collect::<HashSet<_>>();
        Self {
            windows,
            represented_call_ids,
            represented_paths,
        }
    }

    fn prompt_items(&self) -> Vec<NativePromptItem> {
        self.windows
            .iter()
            .map(|window| NativePromptItem {
                item_id: format!("code-window:{}", window.path),
                kind: "code_window".to_string(),
                text: window.render_for_prompt(),
            })
            .collect()
    }
}

#[allow(clippy::too_many_lines)]
pub(crate) fn assemble_native_prompt(
    config: &NativeRuntimeConfig,
    input: &ExecutionInput,
    history: &[TurnItem],
    tool_contracts: &[ToolContract],
) -> NativePromptRender {
    let normalized = normalize_turn_items(history);
    let active_code_windows = extract_active_code_windows(&normalized);
    let base_runtime_instructions = base_runtime_instructions();
    let mode_contract = mode_contract(config.agent_mode);
    let objective_state = objective_state(config.agent_mode, input);
    let objective_state_chars = objective_state.chars().count();
    let metadata = input.native_prompt_metadata.clone().unwrap_or_default();
    let available_budget = config
        .token_budget
        .saturating_sub(config.prompt_headroom.max(1));
    let mode_contract_hash = stable_hash(&mode_contract);
    let tool_contract_lines = tool_contracts
        .iter()
        .map(render_tool_contract)
        .collect::<Vec<_>>();

    let static_sections = [
        section("Base Runtime Instructions", &base_runtime_instructions),
        section("Mode Contract", &mode_contract),
        section("Objective State", &objective_state),
        render_metadata_section(&metadata),
        section(
            "Tool Contracts",
            &join_lines(&tool_contract_lines, "(no tool contracts available)"),
        ),
    ]
    .join("\n\n");
    let static_prompt_chars = static_sections.chars().count();
    let static_budget = static_sections.chars().count().saturating_add(64);
    let item_budget = available_budget.saturating_sub(static_budget);
    let active_code_windows = select_active_code_windows(&active_code_windows, item_budget / 2);
    let active_code_window_items = active_code_windows.prompt_items();
    let active_code_window_count = active_code_window_items.len();
    let active_code_window_chars = active_code_window_items
        .iter()
        .map(|item| item.text.chars().count())
        .sum::<usize>();
    let normalized = prepare_items_for_prompt(normalized, &active_code_windows);
    let selection = select_items_with_budget(
        &normalized,
        item_budget.saturating_sub(active_code_window_chars),
    );
    let selected_items = selection.items;

    let prompt_items = selected_items
        .iter()
        .map(|item| NativePromptItem {
            item_id: item.id.clone(),
            kind: item.kind.kind_name().to_string(),
            text: item.render_for_prompt(),
        })
        .collect::<Vec<_>>();
    let code_navigation = prompt_items
        .iter()
        .filter(|item| item.kind == "code_navigation")
        .cloned()
        .collect::<Vec<_>>();
    let compacted_summaries = prompt_items
        .iter()
        .filter(|item| item.kind == "compacted_summary")
        .cloned()
        .collect::<Vec<_>>();
    let selected_history = prompt_items
        .iter()
        .filter(|item| item.kind != "code_navigation" && item.kind != "compacted_summary")
        .cloned()
        .collect::<Vec<_>>();
    let selected_history_count = selected_history.len();
    let selected_history_chars = selected_history
        .iter()
        .map(|item| item.text.chars().count())
        .sum();
    let code_navigation_count = code_navigation.len();
    let code_navigation_chars = code_navigation
        .iter()
        .map(|item| item.text.chars().count())
        .sum();
    let compacted_summary_count = compacted_summaries.len();
    let compacted_summary_chars = compacted_summaries
        .iter()
        .map(|item| item.text.chars().count())
        .sum();
    let tool_contract_count = tool_contracts.len();
    let tool_contract_chars = tool_contract_lines
        .iter()
        .map(|line| line.chars().count())
        .sum();
    let tool_result_items_visible = selected_items
        .iter()
        .filter(|item| matches!(item.kind, TurnItemKind::ToolResult { .. }))
        .count();
    let latest_tool_result_turn_index =
        selected_items
            .iter()
            .rev()
            .find_map(|item| match item.kind {
                TurnItemKind::ToolResult { .. } => item.provenance.turn_index,
                _ => None,
            });
    let latest_tool_names_visible = selected_items
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolResult { tool_name, .. } => Some(tool_name.clone()),
            _ => None,
        })
        .fold(Vec::<String>::new(), |mut acc, tool_name| {
            if !acc.iter().any(|existing| existing == &tool_name) {
                acc.push(tool_name);
            }
            acc
        });

    let mut sections = vec![static_sections];
    if !active_code_window_items.is_empty() {
        sections.push(section(
            "Active Code Windows",
            &active_code_window_items
                .iter()
                .map(|item| item.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n"),
        ));
    }
    if !selected_history.is_empty() {
        sections.push(section(
            "Runtime-local History",
            &selected_history
                .iter()
                .map(|item| item.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    if !code_navigation.is_empty() {
        sections.push(section(
            "Code Navigation Session",
            &code_navigation
                .iter()
                .map(|item| item.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    if !compacted_summaries.is_empty() {
        sections.push(section(
            "Compacted Summaries",
            &compacted_summaries
                .iter()
                .map(|item| item.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        ));
    }
    sections.push("Return exactly one directive line now.".to_string());
    let prompt = sections.join("\n\n");
    let rendered_prompt_hash = digest_hex(prompt.as_bytes());
    let rendered_prompt_bytes = prompt.len();
    let rendered_prompt_chars = prompt.chars().count();
    let budget_remaining_after_render = available_budget.saturating_sub(rendered_prompt_chars);
    let overflow_classification = if selection.truncated_item_count > 0 {
        "item_truncated"
    } else if selection.skipped_item_count > 0 {
        "history_compacted"
    } else if budget_remaining_after_render <= config.prompt_headroom.min(128) {
        "near_limit"
    } else {
        "within_budget"
    }
    .to_string();

    NativePromptRender {
        prompt,
        assembly: NativePromptAssembly {
            base_runtime_instructions,
            mode_contract,
            mode_contract_hash,
            objective_state,
            selected_history,
            active_code_windows: active_code_window_items,
            code_navigation,
            tool_contracts: tool_contract_lines,
            compacted_summaries,
            prompt_headroom: config.prompt_headroom,
            available_budget,
            rendered_prompt_hash,
            rendered_prompt_bytes,
            selected_item_count: prompt_items.len() + active_code_window_count,
            static_prompt_chars,
            objective_state_chars,
            selected_history_chars,
            active_code_window_chars,
            compacted_summary_chars,
            code_navigation_chars,
            tool_contract_chars,
            assembly_duration_ms: 0,
            selected_history_count,
            active_code_window_count,
            code_navigation_count,
            compacted_summary_count,
            tool_contract_count,
            runtime_context_bytes: metadata.runtime_context_bytes,
            skipped_item_count: selection.skipped_item_count,
            truncated_item_count: selection.truncated_item_count,
            tool_result_items_visible,
            latest_tool_result_turn_index,
            latest_tool_names_visible,
            manifest_hash: metadata.manifest_hash,
            inputs_hash: metadata.inputs_hash,
            delivered_context_hash: metadata.delivered_context_hash,
            rendered_context_hash: metadata.rendered_context_hash,
            context_window_state_hash: metadata.context_window_state_hash,
            delivery_target: metadata.delivery_target,
            overflow_classification,
        },
    }
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn prepare_items_for_prompt(
    items: Vec<TurnItem>,
    active_code_windows: &ActiveCodeWindows,
) -> Vec<TurnItem> {
    let latest_assistant_turn_index = items.iter().filter_map(|item| match item.kind {
        TurnItemKind::AssistantText { .. } => item.provenance.turn_index,
        _ => None,
    });
    let latest_assistant_turn_index = latest_assistant_turn_index.max();

    let read_requests_by_call_id: HashMap<String, String> = items
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolCall {
                call_id,
                tool_name,
                request,
            } if tool_name == "read_file" => Some((call_id.clone(), request.clone())),
            _ => None,
        })
        .collect();

    let mut seen_read_results = HashSet::new();
    let mut prepared = Vec::with_capacity(items.len());
    for item in items.into_iter().rev() {
        if is_represented_by_active_code_window(&item, active_code_windows) {
            continue;
        }
        if should_drop_stale_tool_call(&item, latest_assistant_turn_index) {
            continue;
        }
        if let Some(dedupe_key) = duplicate_read_result_key(&item, &read_requests_by_call_id) {
            if !seen_read_results.insert(dedupe_key) {
                continue;
            }
        }
        prepared.push(item);
    }
    prepared.reverse();
    prepared
}

fn extract_active_code_windows(items: &[TurnItem]) -> ActiveCodeWindows {
    let mut windows_by_path = HashMap::<String, ActiveCodeWindow>::new();
    let mut read_requests_by_call_id = HashMap::<String, String>::new();
    let successful_write_call_ids = items
        .iter()
        .filter_map(|item| match &item.kind {
            TurnItemKind::ToolResult {
                call_id,
                tool_name,
                outcome: TurnItemOutcome::Success,
                ..
            } if tool_name == "write_file" => Some(call_id.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();

    for item in items {
        match &item.kind {
            TurnItemKind::ToolCall {
                call_id,
                tool_name,
                request,
            } if tool_name == "read_file" => {
                if let Some(path) = json_request_string_field(request, "path") {
                    read_requests_by_call_id.insert(call_id.clone(), path);
                }
            }
            TurnItemKind::ToolResult {
                call_id,
                tool_name,
                outcome: TurnItemOutcome::Success,
                content,
            } if tool_name == "read_file" => {
                if let Some(path) = read_requests_by_call_id.get(call_id) {
                    let previous = windows_by_path.get(path);
                    windows_by_path.insert(
                        path.clone(),
                        build_read_code_window(
                            previous,
                            path,
                            call_id,
                            content,
                            item.provenance.turn_index,
                        ),
                    );
                }
            }
            TurnItemKind::ToolCall {
                call_id,
                tool_name,
                request,
            } if tool_name == "write_file" && successful_write_call_ids.contains(call_id) => {
                if let (Some(path), Some(content)) = (
                    json_request_string_field(request, "path"),
                    json_request_string_field(request, "content"),
                ) {
                    let previous = windows_by_path.get(&path);
                    windows_by_path.insert(
                        path.clone(),
                        build_write_code_window(
                            previous,
                            &path,
                            call_id,
                            &content,
                            item.provenance.turn_index,
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    let mut windows = windows_by_path.into_values().collect::<Vec<_>>();
    windows.sort_by_key(|window| std::cmp::Reverse(window.last_turn_index.unwrap_or_default()));
    ActiveCodeWindows::from_windows(windows)
}

fn select_active_code_windows(
    active_code_windows: &ActiveCodeWindows,
    budget: usize,
) -> ActiveCodeWindows {
    let mut windows = active_code_windows.windows.clone();
    windows.sort_by_key(|window| {
        (
            window.status != "dirty",
            std::cmp::Reverse(window.last_turn_index.unwrap_or_default()),
        )
    });

    let mut used = 0usize;
    let mut selected = Vec::new();
    for window in windows {
        let chars = window.render_for_prompt().chars().count();
        if selected.is_empty() || used.saturating_add(chars) <= budget {
            used = used.saturating_add(chars);
            selected.push(window);
        }
    }
    ActiveCodeWindows::from_windows(selected)
}

fn build_read_code_window(
    previous: Option<&ActiveCodeWindow>,
    path: &str,
    call_id: &str,
    content: &str,
    last_turn_index: Option<u32>,
) -> ActiveCodeWindow {
    let current_digest = stable_hash(content);
    let freshness = match previous {
        None => "initial_read".to_string(),
        Some(window) if window.current_digest == current_digest && window.status == "dirty" => {
            "confirmed_write".to_string()
        }
        Some(window) if window.current_digest == current_digest => "reread_unchanged".to_string(),
        Some(_) => "reread_changed".to_string(),
    };
    let changed_lines = match previous {
        Some(window) if window.current_digest != current_digest => {
            compute_changed_line_ranges(&window.current_content, content)
        }
        _ => Vec::new(),
    };
    ActiveCodeWindow {
        path: path.to_string(),
        source_tool: "read_file".to_string(),
        originating_call_id: call_id.to_string(),
        represented_call_ids: merge_represented_call_ids(previous, call_id),
        status: "clean".to_string(),
        freshness,
        current_digest,
        current_content: content.to_string(),
        changed_lines,
        last_turn_index,
    }
}

fn build_write_code_window(
    previous: Option<&ActiveCodeWindow>,
    path: &str,
    call_id: &str,
    content: &str,
    last_turn_index: Option<u32>,
) -> ActiveCodeWindow {
    let current_digest = stable_hash(content);
    let (status, freshness, changed_lines) = match previous {
        None => (
            "dirty".to_string(),
            "initial_write".to_string(),
            compute_changed_line_ranges("", content),
        ),
        Some(window) if window.current_digest == current_digest => (
            window.status.clone(),
            if window.status == "dirty" {
                "write_unchanged_dirty".to_string()
            } else {
                "write_unchanged_clean".to_string()
            },
            Vec::new(),
        ),
        Some(window) => (
            "dirty".to_string(),
            if window.status == "dirty" {
                "write_updates_dirty".to_string()
            } else {
                "write_supersedes_read".to_string()
            },
            compute_changed_line_ranges(&window.current_content, content),
        ),
    };
    ActiveCodeWindow {
        path: path.to_string(),
        source_tool: "write_file".to_string(),
        originating_call_id: call_id.to_string(),
        represented_call_ids: merge_represented_call_ids(previous, call_id),
        status,
        freshness,
        current_digest,
        current_content: content.to_string(),
        changed_lines,
        last_turn_index,
    }
}

fn merge_represented_call_ids(previous: Option<&ActiveCodeWindow>, call_id: &str) -> Vec<String> {
    let mut represented_call_ids = previous
        .map(|window| window.represented_call_ids.clone())
        .unwrap_or_default();
    if !represented_call_ids
        .iter()
        .any(|existing| existing == call_id)
    {
        represented_call_ids.push(call_id.to_string());
    }
    represented_call_ids
}

fn is_represented_by_active_code_window(
    item: &TurnItem,
    active_code_windows: &ActiveCodeWindows,
) -> bool {
    match &item.kind {
        TurnItemKind::ToolCall {
            call_id, tool_name, ..
        }
        | TurnItemKind::ToolResult {
            call_id, tool_name, ..
        } if matches!(tool_name.as_str(), "read_file" | "write_file") => {
            active_code_windows.represented_call_ids.contains(call_id)
        }
        TurnItemKind::AssistantText { directive, content } if directive == "act" => {
            assistant_action_path(content).is_some_and(|(tool_name, path)| {
                matches!(tool_name.as_str(), "read_file" | "write_file")
                    && active_code_windows.represented_paths.contains(&path)
            })
        }
        _ => false,
    }
}

fn assistant_action_path(action: &str) -> Option<(String, String)> {
    let mut parts = action.splitn(3, ':');
    let prefix = parts.next()?;
    if prefix != "tool" {
        return None;
    }
    let tool_name = parts.next()?.to_string();
    let payload = parts.next().unwrap_or_default();
    let path = json_request_string_field(payload, "path")?;
    Some((tool_name, path))
}

fn json_request_string_field(request: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(request)
        .ok()?
        .get(key)?
        .as_str()
        .map(ToString::to_string)
}

fn short_hash(value: &str) -> String {
    value.chars().take(12).collect()
}

fn compute_changed_line_ranges(previous: &str, current: &str) -> Vec<(usize, usize)> {
    if previous == current {
        return Vec::new();
    }

    let previous_lines = previous.lines().collect::<Vec<_>>();
    let current_lines = current.lines().collect::<Vec<_>>();

    let mut prefix = 0;
    while prefix < previous_lines.len()
        && prefix < current_lines.len()
        && previous_lines[prefix] == current_lines[prefix]
    {
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < previous_lines.len().saturating_sub(prefix)
        && suffix < current_lines.len().saturating_sub(prefix)
        && previous_lines[previous_lines.len() - 1 - suffix]
            == current_lines[current_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let changed_start = prefix + 1;
    let changed_end = current_lines
        .len()
        .saturating_sub(suffix)
        .max(changed_start);
    vec![(changed_start, changed_end)]
}

fn should_drop_stale_tool_call(item: &TurnItem, latest_assistant_turn_index: Option<u32>) -> bool {
    let Some(latest_assistant_turn_index) = latest_assistant_turn_index else {
        return false;
    };
    matches!(item.kind, TurnItemKind::ToolCall { .. })
        && item
            .provenance
            .turn_index
            .is_some_and(|turn_index| turn_index < latest_assistant_turn_index)
}

fn duplicate_read_result_key(
    item: &TurnItem,
    read_requests_by_call_id: &HashMap<String, String>,
) -> Option<String> {
    match &item.kind {
        TurnItemKind::ToolResult {
            call_id,
            tool_name,
            outcome: TurnItemOutcome::Success,
            content,
        } if tool_name == "read_file" => Some(format!(
            "{}:{}",
            read_requests_by_call_id
                .get(call_id)
                .map_or("", String::as_str),
            stable_hash(content)
        )),
        _ => None,
    }
}

fn base_runtime_instructions() -> String {
    "You are the Hivemind native runtime controller. Work only from explicit prompt sections, use the declared tools, and keep progress attributable through THINK / ACT / DONE directives. If you would otherwise reply with plain-English planning text like 'I'll inspect the repo first', emit that as THINK instead. When emitting ACT tool:<name>:<payload>, the payload must be a JSON object that matches that tool's input_schema exactly; do not invent fields, do not pass bare strings unless the schema explicitly allows them, and reformulate the action to fit the schema when needed."
        .to_string()
}

fn mode_contract(mode: AgentMode) -> String {
    match mode {
        AgentMode::Planner => "planner: prefer repository inspection and planning actions; do not attempt filesystem mutations or command execution.".to_string(),
        AgentMode::Freeflow => "freeflow: explore iteratively, but stay explicit and attributable when using tools or reaching DONE.".to_string(),
        AgentMode::TaskExecutor => "task_executor: make concrete progress toward the task, verify outcomes with tools when needed, and return DONE only after evidence is sufficient.".to_string(),
    }
}

fn objective_state(mode: AgentMode, input: &ExecutionInput) -> String {
    match mode {
        AgentMode::Planner => format!(
            "planner_target\nTask: {}\nSuccess Criteria: {}",
            input.task_description, input.success_criteria
        ),
        AgentMode::Freeflow => format!(
            "freeflow_goal\nGoal: {}\nExit Signal: {}",
            input.task_description, input.success_criteria
        ),
        AgentMode::TaskExecutor => {
            let kind = if !input.prior_attempts.is_empty() || input.verifier_feedback.is_some() {
                "retry"
            } else {
                "task"
            };
            let checkpoint_guidance = input
                .context
                .as_deref()
                .filter(|context| context.contains("Execution checkpoints"))
                .map(|_| {
                    "\nCheckpoint Handling: keep making task progress; complete declared checkpoints when their step is actually reached, and never let checkpoint management replace the core task steps."
                })
                .unwrap_or("");
            format!(
                "{kind}\nTask: {}\nSuccess Criteria: {}{checkpoint_guidance}",
                input.task_description, input.success_criteria
            )
        }
    }
}

fn render_metadata_section(metadata: &NativePromptMetadata) -> String {
    section(
        "Context Manifest",
        &join_lines(
            &[
                optional_line("manifest_hash", metadata.manifest_hash.as_deref()),
                optional_line("inputs_hash", metadata.inputs_hash.as_deref()),
                optional_line(
                    "delivered_context_hash",
                    metadata.delivered_context_hash.as_deref(),
                ),
                optional_line(
                    "rendered_context_hash",
                    metadata.rendered_context_hash.as_deref(),
                ),
                optional_line(
                    "context_window_state_hash",
                    metadata.context_window_state_hash.as_deref(),
                ),
                optional_line("delivery_target", metadata.delivery_target.as_deref()),
                Some(format!(
                    "runtime_context_bytes={}",
                    metadata.runtime_context_bytes
                )),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
            "(no context manifest metadata)",
        ),
    )
}

fn render_tool_contract(contract: &ToolContract) -> String {
    let permissions = contract
        .required_permissions
        .iter()
        .map(|permission| format!("{permission:?}").to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(",");
    let input_schema = serde_json::to_string(&contract.input_schema)
        .unwrap_or_else(|_| "{\"error\":\"input_schema_unavailable\"}".to_string());
    format!(
        "- {}@{} scope={} permissions={} cancellable={} timeout_ms={} input_schema={}",
        contract.name,
        contract.version,
        contract.required_scope,
        permissions,
        contract.cancellable,
        contract.timeout_ms,
        input_schema,
    )
}

fn select_items_with_budget(items: &[TurnItem], budget: usize) -> NativePromptSelection {
    if budget == 0 {
        return NativePromptSelection {
            items: Vec::new(),
            skipped_item_count: items.iter().filter(|item| item.model_visible).count(),
            truncated_item_count: 0,
        };
    }

    let mut selected = Vec::new();
    let mut used = 0usize;
    let mut skipped_item_count = 0usize;
    let mut truncated_item_count = 0usize;
    for item in items.iter().rev().filter(|item| item.model_visible) {
        let rendered = item.render_for_prompt();
        let chars = rendered.chars().count();
        if used + chars <= budget {
            selected.push(item.clone());
            used += chars;
            continue;
        }
        if selected.is_empty() {
            let mut truncated = item.clone();
            truncated.apply_prompt_truncation(budget);
            if !truncated.render_for_prompt().trim().is_empty() {
                selected.push(truncated);
                truncated_item_count = 1;
            }
        }
        skipped_item_count += 1;
        break;
    }
    selected.reverse();
    NativePromptSelection {
        items: selected,
        skipped_item_count,
        truncated_item_count,
    }
}

#[allow(clippy::manual_map, clippy::option_if_let_else)]
fn optional_line(key: &str, value: Option<&str>) -> Option<String> {
    match value {
        Some(value) => Some(format!("{key}={value}")),
        None => None,
    }
}

fn section(title: &str, body: &str) -> String {
    format!("{title}:\n{body}")
}

fn join_lines(lines: &[String], fallback: &str) -> String {
    if lines.is_empty() {
        fallback.to_string()
    } else {
        lines.join("\n")
    }
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
