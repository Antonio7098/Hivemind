use super::*;
use crate::core::diff::Baseline;

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

pub(crate) fn split_command_string(raw: &str) -> Option<(String, Vec<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || !trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let command = parts.next()?.to_string();
    let args = parts.map(ToString::to_string).collect::<Vec<_>>();
    if args.is_empty() {
        None
    } else {
        Some((command, args))
    }
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

pub(crate) fn capture_worktree_baseline(worktree: &Path) -> std::io::Result<Baseline> {
    Baseline::capture(worktree)
}

#[must_use]
pub(crate) fn changed_worktree_file_paths(previous: &Baseline, current: &Baseline) -> Vec<PathBuf> {
    let mut changed = Vec::new();

    for (path, old_snapshot) in &previous.files {
        match current.files.get(path) {
            Some(new_snapshot) => {
                if !old_snapshot.is_dir
                    && !new_snapshot.is_dir
                    && old_snapshot.hash != new_snapshot.hash
                {
                    changed.push(path.clone());
                }
            }
            None => {
                if !old_snapshot.is_dir {
                    changed.push(path.clone());
                }
            }
        }
    }

    for (path, new_snapshot) in &current.files {
        if !previous.files.contains_key(path) && !new_snapshot.is_dir {
            changed.push(path.clone());
        }
    }

    changed.sort();
    changed.dedup();
    changed
}

pub(crate) fn capture_worktree_dirty_paths(
    worktree: &Path,
    previous: &Baseline,
) -> std::io::Result<(Vec<PathBuf>, Baseline)> {
    let current = capture_worktree_baseline(worktree)?;
    let changed = changed_worktree_file_paths(previous, &current);
    Ok((changed, current))
}
