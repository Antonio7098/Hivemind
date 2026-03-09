use super::OpenRouterModelClient;
use crate::native::ModelTurnRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(super) const NATIVE_DIRECTIVE_SYSTEM_PROMPT: &str = "You are the Hivemind native runtime controller.\nReturn exactly one directive per response with one of these formats:\n- THINK:<short reasoning>\n- ACT:tool:<tool_name>:<json_object>\n- DONE:<summary>\nRules:\n- Never return markdown, code fences, or prose outside the directive line.\n- If you would otherwise reply with plain-English planning text, emit THINK instead.\n- For ACT, always use tool syntax accepted by Hivemind: tool:<name>:<json>.\n- Only use built-in tool names: read_file, list_files, write_file, run_command, checkpoint_complete, exec_command, write_stdin, git_status, git_diff, graph_query.\n- Do not invent tool names.\n- Prefer ACT over DONE while required work remains.\n- Do not return DONE on the first turn unless the user explicitly requested a no-op.\n- When the user prompt defines explicit steps, execute them in order before DONE.\n- Return DONE only after the requested deliverable has been created or verified.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct OpenRouterMessage {
    role: String,
    content: String,
}

impl OpenRouterMessage {
    pub(super) fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub(super) fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }
}

impl OpenRouterModelClient {
    pub(super) fn normalize_model_id(model: impl Into<String>) -> String {
        let model = model.into();
        model
            .strip_prefix("openrouter/")
            .map_or_else(|| model.clone(), ToString::to_string)
    }

    pub(super) fn user_prompt_for_turn(request: &ModelTurnRequest) -> String {
        let mut prompt = format!(
            "Turn index: {}\nCurrent state: {}\nAgent mode: {}\nTask prompt:\n{}\n",
            request.turn_index,
            request.state.as_str(),
            request.agent_mode.as_str(),
            request.prompt
        );
        if let Some(context) = request.context.as_ref() {
            prompt.push_str("\nAdditional context:\n");
            prompt.push_str(context);
            prompt.push('\n');
        }
        prompt.push_str("\nReturn one directive line now.");
        prompt
    }

    pub(super) fn extract_text_content(body: &Value) -> Option<String> {
        let content = body
            .get("choices")?
            .as_array()?
            .first()?
            .get("message")?
            .get("content")?;

        if let Some(text) = content.as_str() {
            return Some(text.to_string());
        }

        let segments = content.as_array()?;
        let mut merged = String::new();
        for segment in segments {
            if let Some(text) = segment.get("text").and_then(Value::as_str) {
                if !merged.is_empty() {
                    merged.push('\n');
                }
                merged.push_str(text);
            }
        }
        if merged.trim().is_empty() {
            None
        } else {
            Some(merged)
        }
    }

    pub(super) fn api_error_message(body: &Value) -> Option<String> {
        body.get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    pub(super) fn normalize_directive(raw: &str) -> String {
        let trimmed = raw.trim().trim_matches('`').trim();
        for line in trimmed.lines() {
            let line = line.trim().trim_matches('`').trim();
            if line.starts_with("THINK:") || line.starts_with("ACT:") || line.starts_with("DONE:") {
                return line.to_string();
            }
        }
        trimmed.to_string()
    }
}
