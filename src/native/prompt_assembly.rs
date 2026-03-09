use super::*;
use crate::adapters::runtime::{ExecutionInput, NativePromptMetadata};
use crate::native::tool_engine::ToolContract;
use crate::native::turn_items::TurnItemKind;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativePromptAssembly {
    pub base_runtime_instructions: String,
    pub mode_contract: String,
    pub mode_contract_hash: String,
    pub objective_state: String,
    #[serde(default)]
    pub selected_history: Vec<NativePromptItem>,
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
    pub selected_history_chars: usize,
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

pub(crate) fn assemble_native_prompt(
    config: &NativeRuntimeConfig,
    input: &ExecutionInput,
    history: &[TurnItem],
    tool_contracts: &[ToolContract],
) -> NativePromptRender {
    let normalized = normalize_turn_items(history);
    let base_runtime_instructions = base_runtime_instructions();
    let mode_contract = mode_contract(config.agent_mode);
    let objective_state = objective_state(config.agent_mode, input);
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
    let selection = select_items_with_budget(&normalized, item_budget);
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

    NativePromptRender {
        prompt,
        assembly: NativePromptAssembly {
            base_runtime_instructions,
            mode_contract,
            mode_contract_hash,
            objective_state,
            selected_history,
            code_navigation,
            tool_contracts: tool_contract_lines,
            compacted_summaries,
            prompt_headroom: config.prompt_headroom,
            available_budget,
            rendered_prompt_hash,
            rendered_prompt_bytes,
            selected_item_count: prompt_items.len(),
            static_prompt_chars,
            selected_history_chars,
            compacted_summary_chars,
            code_navigation_chars,
            tool_contract_chars,
            assembly_duration_ms: 0,
            selected_history_count,
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
        },
    }
}

fn stable_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
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
            } else if input
                .context
                .as_deref()
                .is_some_and(|context| context.contains("Execution checkpoints"))
            {
                "checkpoint"
            } else {
                "task"
            };
            format!(
                "{kind}\nTask: {}\nSuccess Criteria: {}",
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
        .map(|permission| format!("{:?}", permission).to_ascii_lowercase())
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

fn optional_line(key: &str, value: Option<&str>) -> Option<String> {
    value.map(|value| format!("{key}={value}"))
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
