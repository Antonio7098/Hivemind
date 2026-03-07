const REDACTED_SECRET: &str = "[REDACTED]";

pub(super) fn redact_events_for_stream(
    events: Vec<crate::core::events::Event>,
) -> Vec<crate::core::events::Event> {
    events.into_iter().map(redact_stream_event).collect()
}

fn redact_stream_event(event: crate::core::events::Event) -> crate::core::events::Event {
    let Ok(mut raw) = serde_json::to_value(&event) else {
        return event;
    };
    redact_json_secrets(&mut raw, None);
    serde_json::from_value(raw).unwrap_or(event)
}

fn redact_json_secrets(value: &mut serde_json::Value, parent_key: Option<&str>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if is_sensitive_json_key(key) {
                    *child = serde_json::Value::String(REDACTED_SECRET.to_string());
                    continue;
                }
                redact_json_secrets(child, Some(key));
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                redact_json_secrets(child, parent_key);
            }
        }
        serde_json::Value::String(raw) => {
            if parent_key.is_some_and(is_sensitive_json_key) {
                *raw = REDACTED_SECRET.to_string();
            } else {
                *raw = redact_inline_secret_tokens(raw);
            }
        }
        _ => {}
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase();
    [
        "api_key",
        "authorization",
        "access_token",
        "refresh_token",
        "token",
        "secret",
        "password",
        "credential",
        "private_key",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn redact_inline_secret_tokens(raw: &str) -> String {
    let mut current = raw.to_string();
    for (prefix, min_len) in [("sk-or-v1-", 12_usize), ("sk-proj-", 12), ("sk-", 20)] {
        current = redact_prefixed_token(&current, prefix, min_len);
    }
    current
}

fn redact_prefixed_token(input: &str, prefix: &str, min_suffix_len: usize) -> String {
    if !input.contains(prefix) {
        return input.to_string();
    }

    let mut redacted = String::with_capacity(input.len());
    let mut index = 0_usize;
    while index < input.len() {
        if input[index..].starts_with(prefix) {
            let start = index;
            let mut end = start + prefix.len();
            while end < input.len() {
                let ch = input[end..].chars().next().unwrap_or_default();
                if ch.is_whitespace()
                    || matches!(
                        ch,
                        '"' | '\'' | ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}' | '<' | '>'
                    )
                {
                    break;
                }
                end += ch.len_utf8();
            }
            if end.saturating_sub(start + prefix.len()) >= min_suffix_len {
                redacted.push_str(REDACTED_SECRET);
                index = end;
                continue;
            }
        }

        let ch = input[index..].chars().next().unwrap_or_default();
        redacted.push(ch);
        index += ch.len_utf8();
    }

    redacted
}
