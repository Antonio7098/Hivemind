use super::*;

/// Checks if a path matches any pattern in a list.
pub fn path_matches_any(path: &Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    for pattern in patterns {
        if pattern.contains('*') {
            if pattern == "*" {
                return true;
            }
            if let Some(prefix) = pattern.strip_suffix("/*") {
                if path_str.starts_with(prefix) {
                    return true;
                }
            }
            if let Some(suffix) = pattern.strip_prefix("*/") {
                if path_str.ends_with(suffix) {
                    return true;
                }
            }
        } else if path_str.starts_with(pattern) {
            return true;
        }
    }
    false
}
