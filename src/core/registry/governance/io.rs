use super::*;

impl Registry {
    pub(crate) fn validate_governance_identifier(
        raw: &str,
        field: &str,
        origin: &'static str,
    ) -> Result<String> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(HivemindError::user(
                "invalid_governance_identifier",
                format!("{field} cannot be empty"),
                origin,
            )
            .with_hint(
                "Use lowercase letters, numbers, '.', '_' or '-' when naming governance artifacts",
            ));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(HivemindError::user(
                "invalid_governance_identifier",
                format!(
                    "{field} '{trimmed}' contains unsupported characters; only [a-zA-Z0-9._-] are allowed"
                ),
                origin,
            )
            .with_hint("Replace unsupported characters with '-', '_' or '.'"));
        }
        Ok(trimmed.to_string())
    }

    pub(crate) fn normalized_string_list(values: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for value in values {
            let item = value.trim();
            if item.is_empty() {
                continue;
            }
            let item_owned = item.to_string();
            if seen.insert(item_owned.clone()) {
                out.push(item_owned);
            }
        }
        out
    }

    pub(crate) fn read_governance_json<T: DeserializeOwned>(
        path: &Path,
        artifact_kind: &str,
        artifact_key: &str,
        origin: &'static str,
    ) -> Result<T> {
        let raw = fs::read_to_string(path).map_err(|e| {
            HivemindError::system("governance_artifact_read_failed", e.to_string(), origin)
                .with_context("path", path.to_string_lossy().to_string())
                .with_context("artifact_kind", artifact_kind.to_string())
                .with_context("artifact_key", artifact_key.to_string())
        })?;
        serde_json::from_str(&raw).map_err(|e| {
            HivemindError::user(
                "governance_artifact_schema_invalid",
                format!("Malformed {artifact_kind} artifact '{artifact_key}': {e}"),
                origin,
            )
            .with_context("path", path.to_string_lossy().to_string())
            .with_hint(
                "Inspect and repair this artifact JSON, or recreate it using the corresponding CLI create/update command",
            )
        })
    }

    pub(crate) fn write_governance_json<T: Serialize>(
        path: &Path,
        value: &T,
        origin: &'static str,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                HivemindError::system("governance_storage_create_failed", e.to_string(), origin)
            })?;
        }
        let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
            HivemindError::system(
                "governance_artifact_serialize_failed",
                e.to_string(),
                origin,
            )
        })?;
        fs::write(path, bytes).map_err(|e| {
            HivemindError::system("governance_artifact_write_failed", e.to_string(), origin)
        })
    }
}
