use super::*;

impl WorktreeManager {
    #[must_use]
    pub fn path_for(&self, flow_id: Uuid, task_id: Uuid) -> PathBuf {
        self.config
            .base_dir
            .join(flow_id.to_string())
            .join(task_id.to_string())
    }

    pub fn inspect(&self, flow_id: Uuid, task_id: Uuid) -> Result<WorktreeStatus> {
        let path = self.path_for(flow_id, task_id);
        if !path.exists() {
            return Ok(WorktreeStatus {
                flow_id,
                task_id,
                path,
                is_worktree: false,
                head_commit: None,
                branch: None,
            });
        }

        let is_worktree = self.is_worktree(&path);
        let head_commit = if is_worktree {
            self.worktree_head(&path).ok()
        } else {
            None
        };
        let branch = if is_worktree {
            match Command::new("git")
                .current_dir(&path)
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
            {
                Ok(o) if o.status.success() => {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                }
                _ => None,
            }
        } else {
            None
        };

        Ok(WorktreeStatus {
            flow_id,
            task_id,
            path,
            is_worktree,
            head_commit,
            branch,
        })
    }

    pub fn list_for_flow(&self, flow_id: Uuid) -> Result<Vec<PathBuf>> {
        let flow_dir = self.config.base_dir.join(flow_id.to_string());
        if !flow_dir.exists() {
            return Ok(Vec::new());
        }

        let mut worktrees = Vec::new();
        for entry in std::fs::read_dir(&flow_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                worktrees.push(entry.path());
            }
        }
        Ok(worktrees)
    }

    pub fn cleanup_flow(&self, flow_id: Uuid) -> Result<()> {
        for path in self.list_for_flow(flow_id)? {
            self.remove(&path)?;
        }
        let flow_dir = self.config.base_dir.join(flow_id.to_string());
        if flow_dir.exists() {
            std::fs::remove_dir_all(&flow_dir)?;
        }
        Ok(())
    }

    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    pub fn config(&self) -> &WorktreeConfig {
        &self.config
    }
}
