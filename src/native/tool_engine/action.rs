use super::*;

pub(crate) fn default_tool_version() -> String {
    TOOL_VERSION_V1.to_string()
}

impl NativeToolAction {
    /// Parses `tool:` directives in one of these forms:
    /// - `tool:<name>:<json-object>`
    /// - `tool:{"name":"...","version":"...","input":{...}}`
    pub fn parse(action: &str) -> Result<Option<Self>, NativeToolEngineError> {
        let Some(raw) = action.trim().strip_prefix("tool:") else {
            return Ok(None);
        };
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool action is missing tool name/payload",
            ));
        }

        if raw.starts_with('{') {
            return serde_json::from_str::<Self>(raw)
                .map(Some)
                .map_err(|error| {
                    NativeToolEngineError::validation(format!(
                        "tool action JSON payload is invalid: {error}"
                    ))
                });
        }

        let mut parts = raw.splitn(2, ':');
        let tool_name = parts.next().unwrap_or_default().trim();
        if tool_name.is_empty() {
            return Err(NativeToolEngineError::validation(
                "tool name cannot be empty",
            ));
        }

        let input = match parts.next().map(str::trim) {
            None | Some("") => json!({}),
            Some(payload) => serde_json::from_str::<Value>(payload).map_err(|error| {
                NativeToolEngineError::validation(format!(
                    "tool input JSON payload is invalid: {error}"
                ))
            })?,
        };

        Ok(Some(Self {
            name: tool_name.to_string(),
            version: TOOL_VERSION_V1.to_string(),
            input,
        }))
    }
}
