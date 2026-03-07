use super::*;

impl Diff {
    /// Computes a diff between a baseline and current state.
    pub fn compute(baseline: &Baseline, current_root: &Path) -> io::Result<Self> {
        let mut current_files = HashMap::new();
        super::baseline::capture_recursive(current_root, current_root, &mut current_files)?;

        let mut changes = Vec::new();

        for (path, old_snapshot) in &baseline.files {
            match current_files.get(path) {
                Some(new_snapshot) => {
                    if old_snapshot.hash != new_snapshot.hash {
                        changes.push(FileChange {
                            path: path.clone(),
                            change_type: ChangeType::Modified,
                            old_hash: old_snapshot.hash.clone(),
                            new_hash: new_snapshot.hash.clone(),
                        });
                    }
                }
                None => {
                    changes.push(FileChange {
                        path: path.clone(),
                        change_type: ChangeType::Deleted,
                        old_hash: old_snapshot.hash.clone(),
                        new_hash: None,
                    });
                }
            }
        }

        for (path, new_snapshot) in &current_files {
            if !baseline.files.contains_key(path) {
                changes.push(FileChange {
                    path: path.clone(),
                    change_type: ChangeType::Created,
                    old_hash: None,
                    new_hash: new_snapshot.hash.clone(),
                });
            }
        }

        changes.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self {
            id: Uuid::new_v4(),
            task_id: None,
            attempt_id: None,
            baseline_id: baseline.id,
            changes,
            computed_at: chrono::Utc::now(),
        })
    }

    /// Attributes this diff to a task.
    #[must_use]
    pub fn for_task(mut self, task_id: Uuid) -> Self {
        self.task_id = Some(task_id);
        self
    }

    /// Attributes this diff to an attempt.
    #[must_use]
    pub fn for_attempt(mut self, attempt_id: Uuid) -> Self {
        self.attempt_id = Some(attempt_id);
        self
    }

    /// Returns true if there are no changes.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the number of changes.
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// Returns changes of a specific type.
    pub fn changes_of_type(&self, change_type: ChangeType) -> Vec<&FileChange> {
        self.changes
            .iter()
            .filter(|c| c.change_type == change_type)
            .collect()
    }

    /// Returns all modified paths.
    pub fn modified_paths(&self) -> Vec<&Path> {
        self.changes.iter().map(|c| c.path.as_path()).collect()
    }
}

/// Computes unified diff between two files.
pub fn unified_diff(old_path: Option<&Path>, new_path: Option<&Path>) -> io::Result<String> {
    use std::process::Command;

    let (old, new) = match (old_path, new_path) {
        (Some(old), Some(new)) => (old.to_str().unwrap_or(""), new.to_str().unwrap_or("")),
        (Some(old), None) => (old.to_str().unwrap_or(""), "/dev/null"),
        (None, Some(new)) => ("/dev/null", new.to_str().unwrap_or("")),
        (None, None) => return Ok(String::new()),
    };

    let output = Command::new("diff").args(["-u", old, new]).output()?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
