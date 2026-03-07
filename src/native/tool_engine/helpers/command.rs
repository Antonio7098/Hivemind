use super::*;

pub(crate) fn normalize_exec_command(command: &str, env_map: &HashMap<String, String>) -> String {
    let normalized = command.trim();
    if !normalized.eq_ignore_ascii_case("python") {
        return normalized.to_string();
    }
    if command_exists_in_path(normalized, env_map) {
        return normalized.to_string();
    }
    if command_exists_in_path("python3", env_map) {
        return "python3".to_string();
    }
    normalized.to_string()
}

pub(crate) fn command_exists_in_path(command: &str, env_map: &HashMap<String, String>) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_path_components = Path::new(trimmed).components().count() > 1;
    if has_path_components || Path::new(trimmed).is_absolute() {
        return resolve_command_path(trimmed, env_map).is_some();
    }

    let path_value = env_lookup(env_map, "PATH")
        .map(str::to_string)
        .or_else(|| env::var("PATH").ok())
        .unwrap_or_default();
    if path_value.trim().is_empty() {
        return false;
    }
    let windows_exts = windows_command_extensions(env_map);

    env::split_paths(&path_value).any(|dir| {
        let candidate = dir.join(trimmed);
        is_executable_path(&candidate)
            || windows_exts
                .iter()
                .any(|ext| is_executable_path(&dir.join(format!("{trimmed}.{ext}"))))
    })
}

pub(crate) fn resolve_command_path(
    command: &str,
    env_map: &HashMap<String, String>,
) -> Option<PathBuf> {
    let candidate = PathBuf::from(command);
    if is_executable_path(&candidate) {
        return Some(candidate);
    }

    for ext in windows_command_extensions(env_map) {
        let ext = ext.trim_start_matches('.');
        let fallback = PathBuf::from(format!("{command}.{ext}"));
        if is_executable_path(&fallback) {
            return Some(fallback);
        }
    }

    None
}

pub(crate) fn env_lookup<'a>(env_map: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    env_map.get(key).map(String::as_str).or_else(|| {
        env_map
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    })
}

pub(crate) fn windows_command_extensions(env_map: &HashMap<String, String>) -> Vec<String> {
    #[cfg(windows)]
    {
        let raw = env_lookup(env_map, "PATHEXT")
            .map(str::to_string)
            .or_else(|| env::var("PATHEXT").ok())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
        return raw
            .split(';')
            .filter_map(|entry| {
                let trimmed = entry.trim().trim_start_matches('.').to_ascii_lowercase();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .collect();
    }

    #[cfg(not(windows))]
    {
        let _ = env_map;
        Vec::new()
    }
}

pub(crate) fn is_executable_path(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .ok()
            .is_some_and(|meta| (meta.permissions().mode() & 0o111) != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
