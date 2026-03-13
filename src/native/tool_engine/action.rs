use super::*;
use serde_json::Map;

pub(crate) fn default_tool_version() -> String {
    TOOL_VERSION_V1.to_string()
}

impl NativeToolAction {
    /// Parses `tool:` directives in one of these forms:
    /// - `tool:<name>:<json-object>`
    /// - `tool:<name> <json-object>`
    /// - `tool:<name>{<json-object>}`
    /// - `tool:{"name":"...","version":"...","input":{...}}`
    pub fn parse(action: &str) -> Result<Option<Self>, NativeToolEngineError> {
        let Some(raw) = parse_tool_prefix(action) else {
            return Ok(None);
        };

        parse_raw_tool_action(raw.trim()).map(Some)
    }
}

fn parse_raw_tool_action(raw: &str) -> Result<NativeToolAction, NativeToolEngineError> {
    let raw = normalize_noisy_tool_action_prefix(raw.trim());
    if raw.is_empty() {
        return Err(NativeToolEngineError::validation(
            "tool action is missing tool name/payload",
        ));
    }

    if let Some(nested_raw) = parse_tool_prefix(raw) {
        if nested_raw.trim().len() < raw.len() {
            return parse_raw_tool_action(nested_raw.trim());
        }
    }

    if let Some(action) = parse_param_tag_wrapped_tool_action(raw)? {
        return Ok(action);
    }

    if raw.starts_with('{') {
        return match parse_json_prefix::<NativeToolAction>(raw, "tool action JSON payload") {
            Ok(action) => Ok(action),
            Err(error) => parse_structured_tool_action_payload(raw)
                .or_else(|| parse_ruby_hash_tool_action(raw))
                .or_else(|| parse_loose_structured_tool_action(raw))
                .ok_or_else(|| {
                    NativeToolEngineError::validation(format!(
                        "tool action JSON payload is invalid: {error}"
                    ))
                }),
        };
    }

    if let Some(action) = parse_wrapper_leaked_structured_tool_action(raw) {
        return Ok(action);
    }

    if let Some(action) = parse_inline_json_tool_action(raw)? {
        return Ok(action);
    }

    if let Some(action) = parse_key_value_structured_tool_action(raw) {
        return Ok(action);
    }

    if let Some(action) = parse_wrapper_leaked_plain_tool_action(raw)? {
        return Ok(action);
    }

    let tool_name_end = raw
        .find(|ch: char| ch == ':' || ch.is_whitespace())
        .unwrap_or(raw.len());
    let tool_name = raw
        .get(..tool_name_end)
        .unwrap_or_default()
        .trim()
        .trim_end_matches(']')
        .trim();
    let tool_name = sanitize_tool_name_token(tool_name);
    if tool_name.is_empty() {
        return Err(NativeToolEngineError::validation(
            "tool name cannot be empty",
        ));
    }

    let mut payload = raw.get(tool_name_end..).unwrap_or_default().trim_start();
    if let Some(rest) = payload.strip_prefix(':') {
        payload = rest.trim_start();
    }

    if payload.starts_with('{') && tool_name.eq_ignore_ascii_case("tool") {
        if let Some(action) = parse_ruby_hash_tool_action(payload)
            .or_else(|| parse_loose_structured_tool_action(payload))
        {
            return Ok(action);
        }
    }

    let input = match payload {
        "" => json!({}),
        payload if payload.starts_with('<') => parse_xml_parameter_payload(tool_name, payload)
            .ok_or_else(|| {
                NativeToolEngineError::validation(
                    "tool input XML payload is invalid: expected one or more <name>value</name> parameters"
                        .to_string(),
                )
            })?,
        payload if payload.starts_with("args") || payload.starts_with("input") => {
            parse_loose_input_value(payload).ok_or_else(|| {
                NativeToolEngineError::validation(
                    "tool input payload is invalid: expected args/input payload to contain JSON or shell-style flags"
                        .to_string(),
                )
            })?
        }
        payload if payload.starts_with('{') => {
            if let Some(input) = parse_loose_input_value(payload) {
                input
            } else {
                parse_json_prefix::<Value>(payload, "tool input JSON payload").map_err(|error| {
                    NativeToolEngineError::validation(format!(
                        "tool input JSON payload is invalid: {error}"
                    ))
                })?
            }
        }
        payload if payload.contains('=') => parse_key_value_input_payload(payload).ok_or_else(|| {
            NativeToolEngineError::validation(
                "tool input payload is invalid: expected key=value pairs to be parseable"
                    .to_string(),
            )
        })?,
        payload => {
            parse_json_prefix::<Value>(payload, "tool input JSON payload").map_err(|error| {
                NativeToolEngineError::validation(format!(
                    "tool input JSON payload is invalid: {error}"
                ))
            })?
        }
    };

    Ok(NativeToolAction {
        name: tool_name.to_string(),
        version: default_tool_version(),
        input,
    })
}

#[derive(Debug, Deserialize)]
struct StructuredToolActionPayload {
    #[serde(default, alias = "name")]
    tool: Option<String>,
    #[serde(default, alias = "input")]
    args: Option<Value>,
    #[serde(default)]
    version: Option<String>,
}

fn parse_wrapper_leaked_structured_tool_action(raw: &str) -> Option<NativeToolAction> {
    let json_start = raw.find('{')?;
    let prefix = raw.get(..json_start)?.trim();
    if prefix.is_empty() || prefix.contains(':') || is_plain_tool_name_token(prefix) {
        return None;
    }
    parse_structured_tool_action_payload(raw.get(json_start..)?)
}

fn parse_inline_json_tool_action(
    raw: &str,
) -> Result<Option<NativeToolAction>, NativeToolEngineError> {
    let Some(json_start) = raw.find('{') else {
        return Ok(None);
    };
    let separator_start = raw
        .find(|ch: char| ch == ':' || ch.is_whitespace())
        .unwrap_or(raw.len());
    if json_start == 0 || json_start > separator_start {
        return Ok(None);
    }

    let tool_name = raw.get(..json_start).unwrap_or_default().trim();
    if !is_plain_tool_name_token(tool_name) {
        return Ok(None);
    }

    let input = parse_json_prefix::<Value>(
        raw.get(json_start..).unwrap_or_default(),
        "tool input JSON payload",
    )
    .map_err(|error| {
        NativeToolEngineError::validation(format!("tool input JSON payload is invalid: {error}"))
    })?;

    Ok(Some(NativeToolAction {
        name: tool_name.to_string(),
        version: default_tool_version(),
        input,
    }))
}

fn parse_wrapper_leaked_plain_tool_action(
    raw: &str,
) -> Result<Option<NativeToolAction>, NativeToolEngineError> {
    let Some(nested_tool_start) = raw.to_ascii_lowercase().find("tool:") else {
        return Ok(None);
    };
    if nested_tool_start == 0 {
        return Ok(None);
    }

    let prefix = raw.get(..nested_tool_start).unwrap_or_default().trim();
    if prefix.is_empty() || prefix.contains('{') || is_plain_tool_name_token(prefix) {
        return Ok(None);
    }

    let nested = raw
        .get(nested_tool_start..)
        .unwrap_or_default()
        .trim_start();
    let Some(nested_raw) = parse_tool_prefix(nested) else {
        return Ok(None);
    };

    parse_raw_tool_action(nested_raw.trim()).map(Some)
}

fn parse_structured_tool_action_payload(raw: &str) -> Option<NativeToolAction> {
    let payload =
        parse_json_prefix::<StructuredToolActionPayload>(raw, "structured tool action").ok()?;
    let name = payload.tool?.trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(NativeToolAction {
        name,
        version: payload.version.unwrap_or_else(default_tool_version),
        input: payload.args.unwrap_or_else(|| json!({})),
    })
}

fn sanitize_tool_name_token(raw: &str) -> &str {
    let mut token = raw.trim().trim_end_matches(']').trim();
    loop {
        if let Some(rest) = token.strip_prefix("<tool_call>") {
            token = rest.trim();
            continue;
        }
        if let Some(rest) = token.strip_prefix("[tool_call]") {
            token = rest.trim();
            continue;
        }
        if let Some(rest) = token.strip_prefix("[tool:") {
            token = rest.trim().trim_end_matches(']').trim();
            continue;
        }
        if let Some(rest) = token.strip_prefix("<tool:") {
            token = rest.trim().trim_end_matches('>').trim();
            continue;
        }
        break;
    }
    token
}

fn parse_loose_structured_tool_action(raw: &str) -> Option<NativeToolAction> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    let name = parse_loose_string_value(find_loose_field_value(trimmed, "tool")?)?;
    let version = find_loose_field_value(trimmed, "version")
        .and_then(parse_loose_string_value)
        .unwrap_or_else(default_tool_version);
    let input = find_loose_field_value(trimmed, "input")
        .or_else(|| find_loose_field_value(trimmed, "args"))
        .and_then(parse_loose_input_value)
        .unwrap_or_else(|| json!({}));

    Some(NativeToolAction {
        name,
        version,
        input,
    })
}

fn parse_param_tag_wrapped_tool_action(
    raw: &str,
) -> Result<Option<NativeToolAction>, NativeToolEngineError> {
    let trimmed = raw.trim();
    let body = strip_ascii_case_prefix_local(trimmed, "ACT")
        .unwrap_or(trimmed)
        .trim();
    let Some(mut params) = parse_param_name_tags(body) else {
        return Ok(None);
    };

    let inline_tool_name = if trimmed.eq_ignore_ascii_case(body) {
        let tool_name_end = trimmed
            .find(|ch: char| ch == ':' || ch.is_whitespace())
            .unwrap_or(trimmed.len());
        let tool_name = trimmed.get(..tool_name_end).unwrap_or_default().trim();
        (!tool_name.is_empty()).then_some(tool_name.to_string())
    } else {
        None
    };

    let tool_name = params
        .remove("tool")
        .and_then(|value| value.as_str().map(ToString::to_string))
        .or(inline_tool_name)
        .ok_or_else(|| {
            NativeToolEngineError::validation(
                "tool action XML payload is missing a tool name parameter",
            )
        })?;

    let input = if let Some(payload) = params.remove("payload") {
        normalize_wrapped_payload_value(payload)?
    } else {
        Value::Object(params)
    };

    Ok(Some(NativeToolAction {
        name: tool_name,
        version: default_tool_version(),
        input,
    }))
}

fn normalize_wrapped_payload_value(payload: Value) -> Result<Value, NativeToolEngineError> {
    match payload {
        Value::String(raw)
            if raw.trim_start().starts_with('{') || raw.trim_start().starts_with('[') =>
        {
            parse_json_prefix::<Value>(raw.trim(), "tool input JSON payload").map_err(|error| {
                NativeToolEngineError::validation(format!(
                    "tool input JSON payload is invalid: {error}"
                ))
            })
        }
        other => Ok(other),
    }
}

fn parse_param_name_tags(raw: &str) -> Option<Map<String, Value>> {
    let mut params = Map::new();
    let mut remaining = raw.trim();

    while let Some(start) = remaining.to_ascii_lowercase().find("<param") {
        let rest = remaining.get(start + 1..)?;
        let open_end = rest.find('>')?;
        let open_tag = rest.get(..open_end)?;
        let name_start = open_tag.to_ascii_lowercase().find("name=")?;
        let after_name = open_tag.get(name_start + 5..)?.trim_start();
        let quote = after_name.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let name_end = after_name.get(1..)?.find(quote)?;
        let name = after_name.get(1..1 + name_end)?.trim();
        if name.is_empty() {
            return None;
        }
        let content_rest = rest.get(open_end + 1..)?;
        let close_index = content_rest.to_ascii_lowercase().find("</param>")?;
        let value = parse_xml_parameter_value(content_rest.get(..close_index)?.trim());
        params.insert(name.to_string(), value);
        remaining = content_rest.get(close_index + "</param>".len()..)?;
    }

    (!params.is_empty()).then_some(params)
}

fn find_loose_field_value<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    let lower = raw.to_ascii_lowercase();
    let key_lower = key.to_ascii_lowercase();
    let mut search_start = 0usize;

    while search_start <= lower.len() {
        let rel = lower.get(search_start..)?.find(&key_lower)?;
        let start = search_start + rel;
        let before = raw.get(..start)?.chars().next_back();
        if before
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '"')
        {
            search_start = start + key.len();
            continue;
        }

        let after_key = raw.get(start + key.len()..)?;
        let trimmed_after_key = after_key.trim_start();
        let consumed_ws = after_key.len().saturating_sub(trimmed_after_key.len());
        let separator_len = if trimmed_after_key.starts_with(':') {
            1
        } else if trimmed_after_key.starts_with("=>") {
            2
        } else {
            search_start = start + key.len();
            continue;
        };

        let value_start = start + key.len() + consumed_ws + separator_len;
        let value = raw.get(value_start..)?.trim_start();
        return extract_loose_value_span(value);
    }

    None
}

fn extract_loose_value_span(raw: &str) -> Option<&str> {
    let trimmed = raw.trim_start();
    let first = trimmed.chars().next()?;
    match first {
        '{' => extract_wrapped_segment(trimmed, '{', '}'),
        '"' | '\'' => {
            let end = trimmed.get(1..)?.find(first)?;
            trimmed.get(..end + 2)
        }
        _ => {
            let end = trimmed
                .find(|ch: char| ch == ',' || ch == '}' || ch == '\n')
                .unwrap_or(trimmed.len());
            trimmed.get(..end).map(str::trim)
        }
    }
}

fn extract_wrapped_segment(raw: &str, open: char, close: char) -> Option<&str> {
    let mut depth = 0usize;
    for (index, ch) in raw.char_indices() {
        if ch == open {
            depth = depth.saturating_add(1);
        } else if ch == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return raw.get(..=index);
            }
        }
    }
    None
}

fn parse_loose_string_value(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    parse_quoted_string(trimmed).or_else(|| {
        let end = trimmed
            .find(|ch: char| ch == ',' || ch == '}' || ch == '\n')
            .unwrap_or(trimmed.len());
        let value = trimmed.get(..end)?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn strip_loose_field_prefix<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    let trimmed = raw.trim_start();
    if trimmed.len() < key.len() || !trimmed.get(..key.len())?.eq_ignore_ascii_case(key) {
        return None;
    }
    let rest = trimmed.get(key.len()..)?.trim_start();
    if let Some(rest) = rest.strip_prefix(':') {
        return Some(rest.trim_start());
    }
    rest.strip_prefix("=>").map(str::trim_start)
}

fn parse_key_value_input_payload(raw: &str) -> Option<Value> {
    let mut parsed = Map::new();
    let mut remaining = raw.trim();

    while !remaining.is_empty() {
        if remaining.starts_with("[/") || remaining.starts_with("</") {
            break;
        }
        let eq = remaining.find('=')?;
        let key = remaining.get(..eq)?.trim();
        if key.is_empty() {
            break;
        }
        let after_eq = remaining.get(eq + 1..)?.trim_start();
        let (value, rest) = parse_key_value_payload_value(after_eq)?;
        parsed.insert(key.replace('-', "_"), value);
        remaining = rest.trim_start();
    }

    (!parsed.is_empty()).then_some(Value::Object(parsed))
}

fn parse_key_value_structured_tool_action(raw: &str) -> Option<NativeToolAction> {
    let Value::Object(mut params) = parse_key_value_input_payload(raw)? else {
        return None;
    };
    let name = params
        .remove("tool")
        .or_else(|| params.remove("name"))
        .and_then(|value| value.as_str().map(ToString::to_string))?;
    if name.trim().is_empty() {
        return None;
    }
    Some(NativeToolAction {
        name,
        version: default_tool_version(),
        input: Value::Object(params),
    })
}

fn parse_key_value_payload_value(raw: &str) -> Option<(Value, &str)> {
    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        let value = Value::String(rest.get(..end)?.to_string());
        let remainder = rest.get(end + 1..).unwrap_or_default();
        return Some((value, remainder));
    }
    let end = raw.find(|ch: char| ch.is_whitespace()).unwrap_or(raw.len());
    let token = raw.get(..end)?.trim();
    let value = match token {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" | "nil" => Value::Null,
        _ => serde_json::from_str::<Value>(token)
            .unwrap_or_else(|_| Value::String(token.to_string())),
    };
    Some((value, raw.get(end..).unwrap_or_default()))
}

fn parse_loose_input_value(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if let Ok(value) = parse_json_prefix::<Value>(trimmed, "tool input JSON payload") {
        return Some(value);
    }
    if let Some(rest) = strip_loose_field_prefix(trimmed, "args")
        .or_else(|| strip_loose_field_prefix(trimmed, "input"))
    {
        return parse_loose_input_value(rest);
    }
    if trimmed.starts_with("--") {
        return Some(Value::Object(parse_cli_flag_object(trimmed)));
    }
    if trimmed.starts_with('{') {
        if let Some(nested) = find_loose_field_value(trimmed, "input")
            .or_else(|| find_loose_field_value(trimmed, "args"))
        {
            return parse_loose_input_value(nested);
        }
        let inner = extract_balanced_braces(trimmed)?;
        if inner.contains("--") {
            return Some(Value::Object(parse_cli_flag_object(inner)));
        }
    }
    None
}

fn strip_ascii_case_prefix_local<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    (raw.len() >= prefix.len() && raw[..prefix.len()].eq_ignore_ascii_case(prefix))
        .then(|| raw.get(prefix.len()..))
        .flatten()
}

fn normalize_noisy_tool_action_prefix(raw: &str) -> &str {
    let mut normalized = raw.trim();
    loop {
        if let Some(rest) = normalized.strip_prefix("ACT ") {
            normalized = rest.trim_start();
            continue;
        }
        if let Some(rest) = normalized.strip_prefix("ACT]") {
            normalized = rest.trim_start();
            continue;
        }
        if let Some(rest) = normalized.strip_prefix("tool]") {
            normalized = rest.trim_start();
            continue;
        }
        break;
    }
    normalized
}

fn parse_ruby_hash_tool_action(raw: &str) -> Option<NativeToolAction> {
    let trimmed = raw.trim();
    if !trimmed.starts_with('{') || !trimmed.contains("=>") {
        return None;
    }

    let tool_key = trimmed.find("tool")?;
    let tool_value = trimmed.get(tool_key..)?;
    let tool_arrow = tool_value.find("=>")?;
    let tool_rest = tool_value.get(tool_arrow + 2..)?.trim_start();
    let tool_name = parse_quoted_string(tool_rest)?;

    let args_key = trimmed.find("args")?;
    let args_value = trimmed.get(args_key..)?;
    let args_arrow = args_value.find("=>")?;
    let args_rest = args_value.get(args_arrow + 2..)?.trim_start();
    let input = if args_rest.starts_with('{') {
        Value::Object(parse_cli_flag_object(extract_balanced_braces(args_rest)?))
    } else {
        json!({})
    };

    Some(NativeToolAction {
        name: tool_name,
        version: default_tool_version(),
        input,
    })
}

fn parse_quoted_string(raw: &str) -> Option<String> {
    let quote = raw.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let end = raw.get(1..)?.find(quote)?;
    raw.get(1..1 + end).map(ToString::to_string)
}

fn extract_balanced_braces(raw: &str) -> Option<&str> {
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

fn parse_cli_flag_object(raw: &str) -> Map<String, Value> {
    let mut parsed = Map::new();
    let mut remaining = raw.trim();

    while let Some(flag_start) = remaining.find("--") {
        let flag_body = remaining.get(flag_start + 2..).unwrap_or_default();
        let key_end = flag_body
            .find(|ch: char| ch.is_whitespace() || ch == ',' || ch == '}')
            .unwrap_or(flag_body.len());
        let key = flag_body.get(..key_end).unwrap_or_default().trim();
        if key.is_empty() {
            break;
        }
        let after_key = flag_body.get(key_end..).unwrap_or_default().trim_start();
        let (value, rest) = parse_cli_flag_value(after_key);
        parsed.insert(key.replace('-', "_"), value);
        remaining = rest;
    }

    parsed
}

fn parse_cli_flag_value(raw: &str) -> (Value, &str) {
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return (Value::Bool(true), "");
    }
    if let Some(value) = trimmed.strip_prefix('"').and_then(|rest| {
        let end = rest.find('"')?;
        Some((
            Value::String(rest.get(..end)?.to_string()),
            rest.get(end + 1..)?,
        ))
    }) {
        return value;
    }
    let end = trimmed
        .find(|ch: char| ch == ',' || ch == '\n' || ch == '}')
        .unwrap_or(trimmed.len());
    let token = trimmed.get(..end).unwrap_or_default().trim();
    let value = match token {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "nil" | "null" => Value::Null,
        _ => serde_json::from_str::<Value>(token)
            .unwrap_or_else(|_| Value::String(token.to_string())),
    };
    (value, trimmed.get(end..).unwrap_or_default())
}

fn parse_xml_parameter_payload(tool_name: &str, raw: &str) -> Option<Value> {
    if let Some(mut params) = parse_param_name_tags(raw) {
        let should_remove_tool_name = params
            .get("tool_name")
            .and_then(Value::as_str)
            .is_some_and(|value: &str| value.eq_ignore_ascii_case(tool_name));
        if should_remove_tool_name {
            params.remove("tool_name");
        }
        if !params.is_empty() {
            return Some(Value::Object(params));
        }
    }

    let mut params = Map::new();
    let mut remaining = raw.trim();

    while let Some(start) = remaining.find('<') {
        let rest = remaining.get(start + 1..)?;
        if rest.starts_with('/') {
            break;
        }
        let open_end = rest.find('>')?;
        let tag_name = rest.get(..open_end)?.trim();
        if tag_name.is_empty()
            || tag_name.contains(char::is_whitespace)
            || tag_name.contains('/')
            || tag_name.contains(':')
        {
            remaining = rest.get(open_end + 1..)?;
            continue;
        }

        let close_tag = format!("</{tag_name}>");
        let content_rest = rest.get(open_end + 1..)?;
        let close_index = content_rest.find(&close_tag)?;
        let value = parse_xml_parameter_value(content_rest.get(..close_index)?.trim());
        params.insert(tag_name.to_string(), value);
        remaining = content_rest.get(close_index + close_tag.len()..)?;
    }

    let should_remove_tool_name = params
        .get("tool_name")
        .and_then(Value::as_str)
        .is_some_and(|value: &str| value.eq_ignore_ascii_case(tool_name));
    if should_remove_tool_name {
        params.remove("tool_name");
    }

    (!params.is_empty()).then_some(Value::Object(params))
}

fn parse_xml_parameter_value(raw: &str) -> Value {
    if raw.is_empty() {
        Value::String(String::new())
    } else if let Ok(value) = serde_json::from_str::<Value>(raw) {
        value
    } else {
        Value::String(raw.to_string())
    }
}

fn is_plain_tool_name_token(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn parse_json_prefix<T: DeserializeOwned>(raw: &str, _label: &str) -> Result<T, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    T::deserialize(&mut deserializer)
}

fn parse_tool_prefix(action: &str) -> Option<&str> {
    let action = action.trim();
    let prefix = action.get(..4)?;
    if !prefix.eq_ignore_ascii_case("tool") {
        return None;
    }
    let rest = action.get(4..)?.trim_start();
    rest.strip_prefix(':')
}
