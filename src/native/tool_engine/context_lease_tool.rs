use super::*;

const MAX_RETAIN_CONTEXT_TURNS: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum RetainContextFloor {
    HandleOnly,
    GraphCard,
    ExactExcerpt,
    ExpandedExcerpt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainContextInput {
    pub(super) path: String,
    #[serde(default)]
    pub(super) turns: Option<u32>,
    #[serde(default)]
    pub(super) floor: Option<RetainContextFloor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct RetainContextOutput {
    pub(super) granted: bool,
    pub(super) path: String,
    pub(super) turns: u32,
    pub(super) floor: String,
    pub(super) reason: String,
}

pub(super) fn handle_retain_context(
    _ctx: &ToolExecutionContext<'_>,
    input: &Value,
    _default_timeout_ms: u64,
) -> Result<Value, NativeToolEngineError> {
    let input = decode_input::<RetainContextInput>(input)?;
    let path = input.path.trim();
    if path.is_empty() {
        return Err(NativeToolEngineError::validation(
            "retain_context requires a non-empty repository-relative path",
        ));
    }
    if Path::new(path).is_absolute() {
        return Err(NativeToolEngineError::validation(
            "retain_context requires a repository-relative path, not an absolute path",
        ));
    }
    let turns = input.turns.unwrap_or(3).clamp(1, MAX_RETAIN_CONTEXT_TURNS);
    let floor = input.floor.unwrap_or(RetainContextFloor::GraphCard);
    serde_json::to_value(RetainContextOutput {
        granted: true,
        path: path.to_string(),
        turns,
        floor: match floor {
            RetainContextFloor::HandleOnly => "handle_only",
            RetainContextFloor::GraphCard => "graph_card",
            RetainContextFloor::ExactExcerpt => "exact_excerpt",
            RetainContextFloor::ExpandedExcerpt => "expanded_excerpt",
        }
        .to_string(),
        reason: "bounded lease granted; prompt assembly will delay downgrade within budget"
            .to_string(),
    })
    .map_err(|error| {
        NativeToolEngineError::execution(format!("failed to encode retain_context output: {error}"))
    })
}
