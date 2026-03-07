use super::*;

impl Registry {
    pub(super) fn prepare_secondary_repositories(
        &self,
        flow: &TaskFlow,
        successful_task_ids: &[Uuid],
        managers: Vec<(String, WorktreeManager)>,
        prepared_target_branch: &str,
        origin: &'static str,
    ) -> Result<Vec<String>> {
        let mut conflicts = Vec::new();

        for (repo_name, manager) in managers {
            let merge_branch = format!("integration/{}/prepare", flow.id);
            let merge_path = Self::recreate_prepare_worktree(
                &manager,
                flow.id,
                &merge_branch,
                prepared_target_branch,
                origin,
            )?;

            for &task_id in successful_task_ids {
                let task_branch = format!("exec/{}/{task_id}", flow.id);
                let task_ref = format!("refs/heads/{task_branch}");
                if !Self::git_ref_exists(&merge_path, &task_ref) {
                    let details = format!("repo {repo_name}: task {task_id}: missing branch");
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                    break;
                }

                let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
                let checkout = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_checkout_failed", e.to_string(), origin)
                    })?;
                if !checkout.status.success() {
                    let details = format!(
                        "repo {repo_name}: task {task_id}: {}",
                        String::from_utf8_lossy(&checkout.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                    break;
                }

                let merge = Command::new("git")
                    .current_dir(&merge_path)
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "merge",
                        "--no-commit",
                        "--no-ff",
                        &task_branch,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_merge_failed", e.to_string(), origin)
                    })?;
                if !merge.status.success() {
                    let details = format!(
                        "repo {repo_name}: task {task_id}: {}",
                        String::from_utf8_lossy(&merge.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                    let _ = Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge", "--abort"])
                        .output();
                    break;
                }

                let merge_in_progress = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["rev-parse", "-q", "--verify", "MERGE_HEAD"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if !merge_in_progress {
                    let _ = Command::new("git")
                        .current_dir(&merge_path)
                        .args(["checkout", &merge_branch])
                        .output();
                    continue;
                }

                let commit_msg = format!(
                    "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {}\nRepository: {}\nTimestamp: {}\nHivemind-Version: {}",
                    flow.id,
                    prepared_target_branch,
                    repo_name,
                    Utc::now().to_rfc3339(),
                    env!("CARGO_PKG_VERSION")
                );
                let commit = Command::new("git")
                    .current_dir(&merge_path)
                    .args([
                        "-c",
                        "user.name=Hivemind",
                        "-c",
                        "user.email=hivemind@example.com",
                        "-c",
                        "commit.gpgsign=false",
                        "commit",
                        "-m",
                        &commit_msg,
                    ])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_commit_failed", e.to_string(), origin)
                    })?;
                if !commit.status.success() {
                    let details = format!(
                        "repo {repo_name}: task {task_id}: {}",
                        String::from_utf8_lossy(&commit.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                    break;
                }

                let _ = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
                    .output();
                let promote = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["merge", "--ff-only", &sandbox_branch])
                    .output()
                    .map_err(|e| {
                        HivemindError::system("git_merge_failed", e.to_string(), origin)
                    })?;
                if !promote.status.success() {
                    let details = format!(
                        "repo {repo_name}: task {task_id}: {}",
                        String::from_utf8_lossy(&promote.stderr).trim()
                    );
                    conflicts.push(details.clone());
                    self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                    break;
                }
            }

            if conflicts.is_empty() {
                let flow_branch = format!("flow/{}", flow.id);
                let _ = Command::new("git")
                    .current_dir(manager.repo_path())
                    .args(["branch", "-f", &flow_branch, &merge_branch])
                    .output();
            }
        }

        Ok(conflicts)
    }
}
