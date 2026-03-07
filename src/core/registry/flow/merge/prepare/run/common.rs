use super::*;

impl Registry {
    pub(super) fn successful_task_ids_in_topological_order(
        flow: &TaskFlow,
        graph: &TaskGraph,
    ) -> Vec<Uuid> {
        graph
            .topological_order()
            .into_iter()
            .filter(|task_id| {
                flow.task_executions
                    .get(task_id)
                    .is_some_and(|e| e.state == TaskExecState::Success)
            })
            .collect()
    }

    pub(super) fn resolve_prepare_target_branch(
        manager: &WorktreeManager,
        target_branch: Option<&str>,
        origin: &'static str,
    ) -> Result<String> {
        let repo_path = manager.repo_path();
        let current_branch = Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map_or_else(
                || "HEAD".to_string(),
                |o| String::from_utf8_lossy(&o.stdout).trim().to_string(),
            );

        let main_exists = Command::new("git")
            .current_dir(repo_path)
            .args(["show-ref", "--verify", "--quiet", "refs/heads/main"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let target = target_branch.map_or_else(
            || {
                if main_exists {
                    "main".to_string()
                } else {
                    current_branch
                }
            },
            ToString::to_string,
        );
        if target == "HEAD" {
            return Err(HivemindError::user(
                "detached_head",
                "Cannot prepare merge from detached HEAD",
                origin,
            )
            .with_hint("Re-run with --target <branch> or checkout a branch"));
        }

        Ok(target)
    }

    pub(super) fn recreate_prepare_worktree(
        manager: &WorktreeManager,
        flow_id: Uuid,
        merge_branch: &str,
        base_ref: &str,
        origin: &'static str,
    ) -> Result<PathBuf> {
        let merge_path = manager
            .config()
            .base_dir
            .join(flow_id.to_string())
            .join("_integration_prepare");

        if merge_path.exists() {
            let _ = Command::new("git")
                .current_dir(manager.repo_path())
                .args([
                    "worktree",
                    "remove",
                    "--force",
                    merge_path.to_str().unwrap_or(""),
                ])
                .output();
            let _ = fs::remove_dir_all(&merge_path);
        }

        if let Some(parent) = merge_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| HivemindError::system("create_dir_failed", e.to_string(), origin))?;
        }

        let add = Command::new("git")
            .current_dir(manager.repo_path())
            .args([
                "worktree",
                "add",
                "-B",
                merge_branch,
                merge_path.to_str().unwrap_or(""),
                base_ref,
            ])
            .output()
            .map_err(|e| HivemindError::system("git_worktree_add_failed", e.to_string(), origin))?;
        if !add.status.success() {
            return Err(HivemindError::git(
                "git_worktree_add_failed",
                String::from_utf8_lossy(&add.stderr).to_string(),
                origin,
            ));
        }

        Ok(merge_path)
    }
}
