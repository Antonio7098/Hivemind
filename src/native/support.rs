use super::*;
use crate::native::tool_engine::NativeToolAction;
use crate::native::tool_engine::ToolPermission;
use serde_json::{Map, Value};

pub(crate) const fn default_prompt_headroom() -> usize {
    8_192
}

impl AgentMode {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Freeflow => "freeflow",
            Self::TaskExecutor => "task_executor",
        }
    }

    #[must_use]
    pub(crate) fn allows_permissions(self, permissions: &[ToolPermission]) -> bool {
        match self {
            Self::Planner => permissions.iter().all(|permission| {
                matches!(
                    permission,
                    ToolPermission::FilesystemRead | ToolPermission::GitRead
                )
            }),
            Self::Freeflow | Self::TaskExecutor => true,
        }
    }
}

impl AgentLoopState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Think => "think",
            Self::Act => "act",
            Self::Done => "done",
        }
    }
}

impl Default for NativeRuntimeConfig {
    fn default() -> Self {
        Self {
            max_turns: 120,
            timeout_budget: Duration::from_secs(300),
            token_budget: 3_000_000,
            prompt_headroom: default_prompt_headroom(),
            agent_mode: AgentMode::TaskExecutor,
            capture_full_payloads: false,
        }
    }
}

impl<T: ModelClient + ?Sized> ModelClient for Box<T> {
    fn complete_turn(&mut self, request: &ModelTurnRequest) -> Result<String, NativeRuntimeError> {
        (**self).complete_turn(request)
    }

    fn take_transport_telemetry(&mut self) -> NativeTransportTelemetry {
        (**self).take_transport_telemetry()
    }
}

impl ModelDirective {
    pub(crate) fn target_state(&self) -> AgentLoopState {
        match self {
            Self::Think { .. } => AgentLoopState::Think,
            Self::Act { .. } => AgentLoopState::Act,
            Self::Done { .. } => AgentLoopState::Done,
        }
    }

    pub(crate) fn parse_relaxed(raw: &str) -> Option<Self> {
        for candidate in directive_candidates(raw) {
            if let Some(directive) = parse_directive_candidate(candidate) {
                return Some(directive);
            }
        }
        infer_freeform_think(raw)
    }
}

fn directive_candidates(raw: &str) -> Vec<&str> {
    let trimmed = raw.trim().trim_matches('`').trim();
    let mut candidates = Vec::new();
    if !trimmed.is_empty() {
        candidates.push(trimmed);
    }
    for line in trimmed.lines() {
        let line = line.trim().trim_matches('`').trim();
        if !line.is_empty() {
            candidates.push(line);
        }
    }
    candidates
}

fn parse_directive_candidate(candidate: &str) -> Option<ModelDirective> {
    let candidate = strip_leading_formatting(candidate);
    if let Some(directive) = parse_wrapped_directive(candidate) {
        return Some(directive);
    }
    if let Some(ModelDirective::Think { .. }) = parse_directive_prefix(candidate) {
        if let Some(directive) = find_embedded_wrapped_directive(candidate)
            .or_else(|| find_followup_action_or_done(candidate).and_then(parse_directive_prefix))
        {
            return Some(directive);
        }
    }
    if let Some(directive) = parse_directive_prefix(candidate) {
        return Some(directive);
    }
    find_embedded_directive(candidate)
        .and_then(parse_directive_prefix)
        .or_else(|| find_embedded_wrapped_directive(candidate))
}

fn find_followup_action_or_done(candidate: &str) -> Option<&str> {
    find_embedded_directive_with_tokens(candidate, &["act:", "done:", "act ", "done "])
}

fn parse_directive_prefix(candidate: &str) -> Option<ModelDirective> {
    let candidate = candidate.trim();
    for kind in ["THINK", "ACT", "DONE"] {
        let Some(rest) = candidate.get(kind.len()..) else {
            continue;
        };
        if !candidate[..kind.len()].eq_ignore_ascii_case(kind) {
            continue;
        }
        let normalized = if let Some(rest) = rest.trim_start().strip_prefix(':') {
            rest.trim()
        } else if rest.is_empty() || rest.starts_with(char::is_whitespace) {
            rest.trim()
        } else {
            continue;
        };
        return Some(match kind {
            "THINK" => ModelDirective::Think {
                message: normalized.to_string(),
            },
            "ACT" => ModelDirective::Act {
                action: normalized.to_string(),
            },
            "DONE" => ModelDirective::Done {
                summary: normalized.to_string(),
            },
            _ => unreachable!(),
        });
    }
    None
}

fn strip_leading_formatting(candidate: &str) -> &str {
    let mut current = candidate.trim();
    loop {
        #[allow(clippy::option_if_let_else)]
        let next = current.strip_prefix("- ").unwrap_or_else(|| {
            if let Some(rest) = current.strip_prefix("* ") {
                rest
            } else if let Some(rest) = current.strip_prefix("• ") {
                rest
            } else {
                strip_numbering_prefix(current).unwrap_or(current)
            }
        });
        if next == current {
            return current.trim();
        }
        current = next.trim_start();
    }
}

fn strip_numbering_prefix(candidate: &str) -> Option<&str> {
    let digit_count = candidate.chars().take_while(char::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }
    let rest = candidate.get(digit_count..)?;
    if let Some(rest) = rest.strip_prefix('.') {
        return Some(rest.trim_start());
    }
    if let Some(rest) = rest.strip_prefix(')') {
        return Some(rest.trim_start());
    }
    None
}

fn find_embedded_directive(candidate: &str) -> Option<&str> {
    find_embedded_directive_with_tokens(
        candidate,
        &["think:", "act:", "done:", "think ", "act ", "done "],
    )
}

fn find_embedded_directive_with_tokens<'a>(candidate: &'a str, tokens: &[&str]) -> Option<&'a str> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let mut best: Option<usize> = None;
    for token in tokens {
        if let Some(index) = lower.find(token) {
            let boundary_ok = index == 0
                || trimmed[..index]
                    .chars()
                    .last()
                    .is_some_and(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
            if boundary_ok && best.is_none_or(|current| index < current) {
                best = Some(index);
            }
        }
    }
    best.and_then(|index| trimmed.get(index..))
}

fn parse_wrapped_directive(candidate: &str) -> Option<ModelDirective> {
    parse_bracketed_assistant_directive(candidate)
        .or_else(|| parse_bracketed_tool_call(candidate))
        .or_else(|| parse_named_tool_wrapper(candidate))
        .or_else(|| parse_xml_tool_call(candidate))
        .or_else(|| parse_minimax_tool_call(candidate))
}

fn find_embedded_wrapped_directive(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    let bracketed = trimmed
        .find("[assistant:")
        .and_then(|index| trimmed.get(index..));
    if let Some(directive) = bracketed.and_then(parse_bracketed_assistant_directive) {
        return Some(directive);
    }
    let tool_call = find_ascii_case_insensitive(trimmed, "[tool_call:")
        .into_iter()
        .chain(find_ascii_case_insensitive(trimmed, "[tool_call]"))
        .min()
        .and_then(|index| trimmed.get(index..));
    if let Some(directive) = tool_call.and_then(parse_bracketed_tool_call) {
        return Some(directive);
    }
    let named_tool_wrapper = find_ascii_case_insensitive(trimmed, "[tool:")
        .into_iter()
        .chain(find_ascii_case_insensitive(trimmed, "<tool:"))
        .min()
        .and_then(|index| trimmed.get(index..));
    if let Some(directive) = named_tool_wrapper.and_then(parse_named_tool_wrapper) {
        return Some(directive);
    }
    if let Some(directive) = trimmed
        .find("<tool_call>")
        .and_then(|index| trimmed.get(index..))
        .and_then(parse_xml_tool_call)
    {
        return Some(directive);
    }
    trimmed
        .find("<minimax:tool_call")
        .and_then(|index| trimmed.get(index..))
        .and_then(parse_minimax_tool_call)
}

fn parse_bracketed_assistant_directive(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    let rest = trimmed.strip_prefix("[assistant:")?;
    let closing = rest.find(']')?;
    let kind = rest.get(..closing)?.trim();
    let payload = rest.get(closing + 1..)?.trim();
    if payload.is_empty() {
        return None;
    }
    match kind.to_ascii_lowercase().as_str() {
        "think" => Some(ModelDirective::Think {
            message: payload.to_string(),
        }),
        "act" => {
            if NativeToolAction::parse(payload).ok().flatten().is_some() {
                Some(ModelDirective::Act {
                    action: payload.to_string(),
                })
            } else {
                None
            }
        }
        "done" => Some(ModelDirective::Done {
            summary: payload.to_string(),
        }),
        _ => None,
    }
}

fn parse_bracketed_tool_call(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    if let Some(rest) = strip_ascii_case_prefix(trimmed, "[tool_call:") {
        let open_end = rest.find(']')?;
        let tool_name = rest.get(..open_end)?.trim();
        let body = rest.get(open_end + 1..)?.trim();
        let action = if body.is_empty() {
            normalize_bracketed_tool_action(tool_name)
        } else if body.starts_with('{') || body.starts_with('[') {
            format!("tool:{tool_name}:{body}")
        } else {
            normalize_bracketed_tool_action(&format!("tool:{tool_name} {body}"))
        };
        return Some(ModelDirective::Act { action });
    }
    let inner = extract_bracketed_tool_call_body(trimmed)?.trim();
    if inner.is_empty() {
        return None;
    }
    Some(ModelDirective::Act {
        action: normalize_bracketed_tool_action(inner),
    })
}

fn parse_named_tool_wrapper(candidate: &str) -> Option<ModelDirective> {
    parse_bracketed_named_tool_wrapper(candidate)
        .or_else(|| parse_xml_named_tool_wrapper(candidate))
}

fn parse_bracketed_named_tool_wrapper(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    let rest = strip_ascii_case_prefix(trimmed, "[tool:")?;
    let open_end = rest.find(']')?;
    let tool_name = rest.get(..open_end)?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let body = rest.get(open_end + 1..)?;
    if let Some(close_index) = find_ascii_case_insensitive(body, "[/tool]") {
        build_named_tool_wrapper_action(tool_name, body.get(..close_index)?)
    } else {
        build_named_tool_wrapper_action(tool_name, body)
    }
}

fn parse_xml_named_tool_wrapper(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    let start = find_ascii_case_insensitive(trimmed, "<tool:")?;
    let rest = trimmed.get(start..)?;
    let open_end = rest.find('>')?;
    let tool_name = rest.get(6..open_end)?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let close_tag = format!("</tool:{tool_name}>");
    let body = rest.get(open_end + 1..)?;
    let close_index = find_ascii_case_insensitive(body, &close_tag)?;
    build_named_tool_wrapper_action(tool_name, body.get(..close_index)?)
}

fn build_named_tool_wrapper_action(tool_name: &str, body: &str) -> Option<ModelDirective> {
    let body = body.trim();
    let action = if body.is_empty() {
        format!("tool:{tool_name}")
    } else if let Some(action) = parse_bracketed_wrapper_argument_action(tool_name, body) {
        action
    } else if let Ok(arguments) = serde_json::from_str::<Value>(body) {
        format!("tool:{tool_name}:{arguments}")
    } else {
        let mut arguments = parse_minimax_tool_call_input(body);
        normalize_recovered_tool_input(tool_name, &mut arguments);
        if arguments.is_empty() {
            return None;
        }
        format!("tool:{tool_name}:{}", Value::Object(arguments))
    };
    Some(ModelDirective::Act { action })
}

fn parse_bracketed_wrapper_argument_action(tool_name: &str, body: &str) -> Option<String> {
    let trimmed = body.trim();
    let rest = strip_ascii_case_prefix(trimmed, "[args:")
        .or_else(|| strip_ascii_case_prefix(trimmed, "[input:"))?
        .trim();
    let content = if let Some(close) = find_ascii_case_insensitive(rest, "[/tool_call]") {
        rest.get(..close)?
    } else if let Some(close) = find_ascii_case_insensitive(rest, "[/tool]") {
        rest.get(..close)?
    } else {
        rest
    };
    let content = content.trim();
    let synthetic = if content.starts_with('{') {
        let inner = extract_balanced_braces_local(content)?;
        let inner = inner.trim();
        if inner.starts_with("--") {
            format!("tool:{tool_name} {inner}")
        } else {
            format!("tool:{tool_name}:{{{inner}}}")
        }
    } else if content.starts_with("--") {
        format!("tool:{tool_name} {content}")
    } else {
        return None;
    };
    Some(normalize_bracketed_tool_action(&synthetic))
}

fn extract_balanced_braces_local(raw: &str) -> Option<&str> {
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        match ch {
            '{' => depth = depth.saturating_add(1),
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return raw.get(1..index);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_bracketed_tool_call_body(candidate: &str) -> Option<&str> {
    let rest = strip_ascii_case_prefix(candidate, "[tool_call]")?;
    if let Some(close) = find_ascii_case_insensitive(rest, "[/tool_call]") {
        rest.get(..close)
    } else {
        Some(rest)
    }
}

fn parse_xml_tool_call(candidate: &str) -> Option<ModelDirective> {
    let trimmed = candidate.trim();
    let function = extract_tag_value(trimmed, "<function=", '>', None)?;
    if !function.eq_ignore_ascii_case("ACT") {
        return Some(ModelDirective::Act {
            action: normalize_tool_action(function),
        });
    }
    let tool_name = extract_tag_value(trimmed, "<parameter=tool>", '\0', Some("</parameter>"))?;
    let arguments = extract_tag_value(trimmed, "<parameter=arguments>", '\0', Some("</parameter>"));
    let action = match arguments.map(|value| value.trim().to_string()) {
        Some(arguments) if !arguments.is_empty() => {
            format!("tool:{}:{}", tool_name.trim(), arguments)
        }
        _ => format!("tool:{}", tool_name.trim()),
    };
    Some(ModelDirective::Act { action })
}

fn parse_minimax_tool_call(candidate: &str) -> Option<ModelDirective> {
    let body = extract_minimax_tool_call_body(candidate)?;
    if let Some(directive) = parse_minimax_invoke_tool_call(body) {
        return Some(directive);
    }
    parse_minimax_wrapped_native_tool_call(body)
}

fn extract_minimax_tool_call_body<'a>(candidate: &'a str) -> Option<&'a str> {
    let trimmed = candidate.trim();
    let start = trimmed.find("<minimax:tool_call")?;
    let rest = trimmed.get(start..)?;
    let open_end = rest.find('>')?;
    let inner = rest.get(open_end + 1..)?.trim();
    if let Some(close_index) = inner.find("</minimax:tool_call>") {
        return inner.get(..close_index).map(str::trim);
    }
    if let Some(close_index) = inner.find("[/tool_call]") {
        return inner.get(..close_index).map(str::trim);
    }
    Some(inner)
}

fn parse_minimax_invoke_tool_call(body: &str) -> Option<ModelDirective> {
    let invoke = extract_xml_element(body, "invoke")?;
    let invoke_open_end = invoke.find('>')?;
    let invoke_open = invoke.get(..=invoke_open_end)?;
    let tool_name = extract_xml_attribute(invoke_open, "name")?;
    let invoke_body = invoke
        .get(invoke_open_end + 1..)?
        .strip_suffix("</invoke>")?
        .trim();
    let input = parse_minimax_tool_call_input(invoke_body);
    let action = if input.is_empty() {
        format!("tool:{tool_name}")
    } else {
        format!("tool:{tool_name}:{}", Value::Object(input))
    };
    Some(ModelDirective::Act { action })
}

fn parse_minimax_wrapped_native_tool_call(body: &str) -> Option<ModelDirective> {
    if let Some(action) = parse_inline_minimax_xml_tool_call(body) {
        return Some(ModelDirective::Act { action });
    }
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(action) = parse_minimax_native_tool_action(trimmed) {
            return Some(ModelDirective::Act { action });
        }
    }
    parse_minimax_native_tool_action(body.trim()).map(|action| ModelDirective::Act { action })
}

fn parse_inline_minimax_xml_tool_call(body: &str) -> Option<String> {
    let start = find_ascii_case_insensitive(body, "<tool_call>")?;
    let rest = body.get(start + "<tool_call>".len()..)?;
    let close = find_ascii_case_insensitive(rest, "</tool_call>")?;
    let inner = rest.get(..close)?.trim();
    if let Some((prefix, json_payload)) = inner.split_once("<json>") {
        let candidate = format!("{}:{}", prefix.trim(), json_payload.trim());
        return parse_minimax_native_tool_action(&candidate);
    }
    parse_minimax_native_tool_action(inner)
}

fn parse_minimax_native_tool_action(candidate: &str) -> Option<String> {
    let candidate = candidate.trim();
    let candidate = candidate
        .strip_prefix(':')
        .map(str::trim_start)
        .unwrap_or(candidate);
    let candidate = if candidate.starts_with("tool:") {
        candidate.to_string()
    } else {
        format!("tool:{candidate}")
    };
    let action = NativeToolAction::parse(&candidate).ok().flatten()?;
    Some(render_tool_action(&action))
}

fn normalize_bracketed_tool_action(raw: &str) -> String {
    parse_shell_style_tool_action(raw).unwrap_or_else(|| normalize_tool_action(raw))
}

fn parse_shell_style_tool_action(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let rest = strip_ascii_case_prefix(trimmed, "tool:")?;
    if rest.contains('{') {
        return None;
    }

    let tool_name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let tool_name = rest.get(..tool_name_end)?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let args = rest.get(tool_name_end..)?.trim();
    if !args.starts_with("--") {
        return None;
    }

    let tokens = split_shell_style_tokens(args);
    if tokens.is_empty() {
        return None;
    }

    let mut input = Map::new();
    let mut index = 0;
    while index < tokens.len() {
        let flag = tokens.get(index)?;
        if !flag.starts_with("--") {
            return None;
        }
        let key = flag.trim_start_matches('-').replace('-', "_");
        if key.is_empty() {
            return None;
        }

        let value = match tokens.get(index + 1) {
            Some(next) if !next.starts_with("--") => {
                index += 2;
                shell_style_value(next)
            }
            _ => {
                index += 1;
                Value::Bool(true)
            }
        };
        input.insert(key, value);
    }

    Some(format!("tool:{tool_name}:{}", Value::Object(input)))
}

fn split_shell_style_tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escape = false;

    for ch in raw.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match quote {
            Some(active_quote) => match ch {
                '\\' => escape = true,
                _ if ch == active_quote => quote = None,
                _ => current.push(ch),
            },
            None => match ch {
                '\'' | '"' => quote = Some(ch),
                '\\' => escape = true,
                _ if ch.is_whitespace() => {
                    if !current.is_empty() {
                        tokens.push(std::mem::take(&mut current));
                    }
                }
                _ => current.push(ch),
            },
        }
    }

    if escape {
        current.push('\\');
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn shell_style_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    match trimmed.to_ascii_lowercase().as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => trimmed
            .parse::<i64>()
            .map_or_else(|_| Value::String(trimmed.to_string()), Value::from),
    }
}

fn render_tool_action(action: &NativeToolAction) -> String {
    if action.input.is_object()
        && action
            .input
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    {
        format!("tool:{}", action.name)
    } else {
        format!("tool:{}:{}", action.name, action.input)
    }
}

fn extract_xml_element<'a>(candidate: &'a str, tag_name: &str) -> Option<&'a str> {
    let open_index = candidate.find(&format!("<{tag_name}"))?;
    let rest = candidate.get(open_index..)?;
    let close_tag = format!("</{tag_name}>");
    let close_index = rest.find(&close_tag)?;
    rest.get(..close_index + close_tag.len())
}

fn extract_xml_attribute(tag: &str, attribute: &str) -> Option<String> {
    let marker = format!("{attribute}=\"");
    let rest = tag.split_once(&marker)?.1;
    let value = rest.split_once('"')?.0.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn parse_minimax_tool_call_input(body: &str) -> Map<String, Value> {
    let named = parse_minimax_named_parameters(body);
    if !named.is_empty() {
        return named;
    }
    parse_minimax_tag_parameters(body)
}

fn parse_minimax_named_parameters(body: &str) -> Map<String, Value> {
    let mut params = Map::new();
    let mut remaining = body;
    while let Some(start) = remaining.find("<parameter") {
        let rest = match remaining.get(start..) {
            Some(rest) => rest,
            None => break,
        };
        let open_end = match rest.find('>') {
            Some(index) => index,
            None => break,
        };
        let open_tag = &rest[..=open_end];
        let Some(name) = extract_xml_attribute(open_tag, "name") else {
            remaining = &rest[open_end + 1..];
            continue;
        };
        let content_start = open_end + 1;
        let Some(content_rest) = rest.get(content_start..) else {
            break;
        };
        let Some(close_index) = content_rest.find("</parameter>") else {
            break;
        };
        let value = content_rest[..close_index].trim();
        params.insert(name, parse_minimax_parameter_value(value));
        remaining = &content_rest[close_index + "</parameter>".len()..];
    }
    params
}

fn parse_minimax_tag_parameters(body: &str) -> Map<String, Value> {
    let mut params = Map::new();
    let mut remaining = body;
    while let Some(start) = remaining.find('<') {
        let rest = match remaining.get(start + 1..) {
            Some(rest) => rest,
            None => break,
        };
        if rest.starts_with('/') {
            break;
        }
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let tag_name = rest[..open_end].trim();
        if tag_name.is_empty()
            || tag_name.contains(char::is_whitespace)
            || tag_name.contains('/')
            || tag_name.contains(':')
        {
            remaining = &rest[open_end + 1..];
            continue;
        }
        let close_tag = format!("</{tag_name}>");
        let content_rest = &rest[open_end + 1..];
        let Some(close_index) = content_rest.find(&close_tag) else {
            break;
        };
        let value = content_rest[..close_index].trim();
        params.insert(tag_name.to_string(), parse_minimax_parameter_value(value));
        remaining = &content_rest[close_index + close_tag.len()..];
    }
    params
}

fn parse_minimax_parameter_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::String(String::new())
    } else if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        value
    } else {
        Value::String(trimmed.to_string())
    }
}

fn normalize_recovered_tool_input(tool_name: &str, input: &mut Map<String, Value>) {
    let should_remove_tool_name = input
        .get("tool_name")
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(tool_name));
    if should_remove_tool_name {
        input.remove("tool_name");
    }
}

fn extract_tag_value<'a>(
    candidate: &'a str,
    prefix: &str,
    end_char: char,
    end_marker: Option<&str>,
) -> Option<&'a str> {
    let rest = candidate.split_once(prefix)?.1;
    let raw = if let Some(marker) = end_marker {
        rest.split_once(marker)?.0
    } else {
        rest.split_once(end_char)?.0
    };
    let value = raw.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn normalize_tool_action(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("tool:"))
    {
        trimmed.to_string()
    } else {
        format!("tool:{trimmed}")
    }
}

fn strip_ascii_case_prefix<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    raw.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .and_then(|_| raw.get(prefix.len()..))
}

fn find_ascii_case_insensitive(raw: &str, needle: &str) -> Option<usize> {
    raw.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

fn infer_freeform_think(raw: &str) -> Option<ModelDirective> {
    let trimmed = raw.trim().trim_matches('`').trim();
    let trimmed = trimmed
        .strip_prefix("<think>")
        .or_else(|| trimmed.strip_prefix("<thinking>"))
        .unwrap_or(trimmed)
        .trim();
    let trimmed = trimmed
        .strip_suffix("</think>")
        .or_else(|| trimmed.strip_suffix("</thinking>"))
        .unwrap_or(trimmed)
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    let first_person_or_planning = [
        "i ", "i'", "let me", "first ", "next ", "i will", "i'll", "plan:",
    ]
    .iter()
    .any(|prefix| lowered.starts_with(prefix));
    if !first_person_or_planning {
        return None;
    }
    Some(ModelDirective::Think {
        message: trimmed.lines().next().unwrap_or(trimmed).trim().to_string(),
    })
}
