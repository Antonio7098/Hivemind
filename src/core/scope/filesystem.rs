use super::*;

impl PathRule {
    /// Creates a new path rule.
    #[must_use]
    pub fn new(pattern: impl Into<String>, permission: FilePermission) -> Self {
        Self {
            pattern: pattern.into(),
            permission,
        }
    }

    /// Creates a read-only path rule.
    #[must_use]
    pub fn read(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Read)
    }

    /// Creates a read-write path rule.
    #[must_use]
    pub fn write(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Write)
    }

    /// Creates a deny path rule.
    #[must_use]
    pub fn deny(pattern: impl Into<String>) -> Self {
        Self::new(pattern, FilePermission::Deny)
    }

    /// Checks if this rule matches a given path.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        if self.pattern.contains('*') {
            glob_match(&self.pattern, path)
        } else {
            path.starts_with(&self.pattern)
        }
    }
}

/// Simple glob matching (supports * and **).
fn glob_match(pattern: &str, path: &str) -> bool {
    if pattern == "**" || pattern == "*" {
        return true;
    }
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');
            if !prefix.is_empty() && !path.starts_with(prefix) {
                return false;
            }
            if !suffix.is_empty() {
                if let Some(ext) = suffix.strip_prefix("*.") {
                    return path.ends_with(&format!(".{ext}"));
                }
                return path.ends_with(suffix);
            }
            return true;
        }
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return path.starts_with(prefix);
    }
    if let Some(suffix) = pattern.strip_prefix("*/") {
        return path.ends_with(suffix);
    }
    pattern == path
}

impl FilesystemScope {
    /// Creates an empty filesystem scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a path rule.
    #[must_use]
    pub fn with_rule(mut self, rule: PathRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Gets the permission for a path.
    #[must_use]
    pub fn permission_for(&self, path: &str) -> Option<FilePermission> {
        let mut has_read = false;
        let mut has_write = false;

        for rule in &self.rules {
            if !rule.matches(path) {
                continue;
            }

            match rule.permission {
                FilePermission::Deny => return Some(FilePermission::Deny),
                FilePermission::Write => has_write = true,
                FilePermission::Read => has_read = true,
            }
        }

        if has_write {
            Some(FilePermission::Write)
        } else if has_read {
            Some(FilePermission::Read)
        } else {
            None
        }
    }

    /// Checks if a path is allowed for writing.
    #[must_use]
    pub fn can_write(&self, path: &str) -> bool {
        matches!(self.permission_for(path), Some(FilePermission::Write))
    }

    /// Checks if a path is allowed for reading.
    #[must_use]
    pub fn can_read(&self, path: &str) -> bool {
        matches!(
            self.permission_for(path),
            Some(FilePermission::Read | FilePermission::Write)
        )
    }
}
