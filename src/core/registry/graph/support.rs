use super::*;

impl Registry {
    pub(crate) fn graph_block_path_value<'a>(
        custom: &'a HashMap<String, serde_json::Value>,
    ) -> Option<&'a str> {
        custom
            .get("path")
            .and_then(serde_json::Value::as_str)
            .filter(|path| !path.trim().is_empty())
            .or_else(|| {
                custom
                    .get("coderef")
                    .and_then(serde_json::Value::as_object)
                    .and_then(|coderef| coderef.get("path"))
                    .and_then(serde_json::Value::as_str)
                    .filter(|path| !path.trim().is_empty())
            })
    }
}
