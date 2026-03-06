use super::*;

pub(super) fn resolve_required_selector(
    positional: Option<&str>,
    flag_value: Option<&str>,
    flag_name: &str,
    noun: &str,
    origin: &str,
) -> Result<String, HivemindError> {
    let pos = positional.map(str::trim).filter(|s| !s.is_empty());
    let flag = flag_value.map(str::trim).filter(|s| !s.is_empty());
    match (pos, flag) {
        (Some(a), Some(b)) if a != b => Err(HivemindError::user(
            "selector_conflict",
            format!("Conflicting {noun} values provided via positional argument and {flag_name}"),
            origin,
        )
        .with_context("positional", a)
        .with_context("flag", b)),
        (Some(a), _) => Ok(a.to_string()),
        (None, Some(b)) => Ok(b.to_string()),
        (None, None) => Err(HivemindError::user(
            "missing_required_selector",
            format!("Provide {noun} as a positional argument or via {flag_name}"),
            origin,
        )),
    }
}

pub(super) fn parse_repo_access_mode(
    value: &str,
    format: OutputFormat,
) -> std::result::Result<RepoAccessMode, ExitCode> {
    match value.to_lowercase().as_str() {
        "ro" => Ok(RepoAccessMode::ReadOnly),
        "rw" => Ok(RepoAccessMode::ReadWrite),
        other => Err(output_error(
            &HivemindError::user(
                "invalid_access_mode",
                format!("Invalid access mode: '{other}'"),
                "cli:project:attach-repo",
            )
            .with_hint("Use --access ro or --access rw"),
            format,
        )),
    }
}
