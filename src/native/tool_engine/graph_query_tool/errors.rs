use super::*;

pub(super) fn graph_query_runtime_error(
    code: &str,
    message: impl Into<String>,
) -> NativeToolEngineError {
    NativeToolEngineError::new(code, message, false)
}
pub(super) fn graph_query_runtime_error_with_refresh_hint(
    code: &str,
    message: impl Into<String>,
) -> NativeToolEngineError {
    graph_query_runtime_error(
        code,
        format!("{}. Hint: {GRAPH_QUERY_REFRESH_HINT}", message.into()),
    )
}
