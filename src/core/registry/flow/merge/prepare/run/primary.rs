#![allow(clippy::too_many_lines)]

use super::*;

pub(super) struct PrimaryPrepareOutcome {
    pub(super) merge_branch: String,
    pub(super) merge_path: PathBuf,
    pub(super) integrated_tasks: Vec<(Uuid, Option<String>)>,
    pub(super) conflicts: Vec<String>,
}

impl Registry {
    pub(super) fn prepare_primary_repository(
        &self,
        flow: &TaskFlow,
        graph: &TaskGraph,
        successful_task_ids: &[Uuid],
        manager: &WorktreeManager,
        prepared_target_branch: &str,
        origin: &'static str,
    ) -> Result<PrimaryPrepareOutcome> {
        let merge_branch = format!("integration/{}/prepare", flow.id);
        let merge_path = Self::recreate_prepare_worktree(
            manager,
            flow.id,
            &merge_branch,
            prepared_target_branch,
            origin,
        )?;

        let mut conflicts = Vec::new();
        let mut integrated_tasks = Vec::new();

        for &task_id in successful_task_ids {
            let task_branch = format!("exec/{}/{task_id}", flow.id);
            let task_ref = format!("refs/heads/{task_branch}");

            let ref_exists = Command::new("git")
                .current_dir(&merge_path)
                .args(["show-ref", "--verify", "--quiet", &task_ref])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ref_exists {
                let details = format!("task {task_id}: missing branch '{task_branch}'");
                conflicts.push(details.clone());
                self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                break;
            }

            let _ = Command::new("git")
                .current_dir(&merge_path)
                .args(["checkout", &merge_branch])
                .output();

            if let Some(deps) = graph.dependencies.get(&task_id) {
                for dep in deps {
                    let dep_branch = format!("exec/{}/{dep}", flow.id);
                    let Some(dep_sha) = Self::resolve_git_ref(&merge_path, &dep_branch) else {
                        let details =
                            format!("task {task_id}: dependency branch missing for {dep_branch}");
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                        break;
                    };

                    let contains_dependency = Command::new("git")
                        .current_dir(&merge_path)
                        .args(["merge-base", "--is-ancestor", &dep_sha, &task_branch])
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if !contains_dependency {
                        let details = format!(
                            "task {task_id}: drift detected (missing prerequisite integrated changes)"
                        );
                        conflicts.push(details.clone());
                        self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                        break;
                    }
                }
                if !conflicts.is_empty() {
                    break;
                }
            }

            let sandbox_branch = format!("integration/{}/{task_id}", flow.id);
            let checkout = Command::new("git")
                .current_dir(&merge_path)
                .args(["checkout", "-B", &sandbox_branch, &merge_branch])
                .output()
                .map_err(|e| HivemindError::system("git_checkout_failed", e.to_string(), origin))?;
            if !checkout.status.success() {
                return Err(HivemindError::git(
                    "git_checkout_failed",
                    String::from_utf8_lossy(&checkout.stderr).to_string(),
                    origin,
                ));
            }

            let merge = Command::new("git")
                .current_dir(&merge_path)
                .env("GIT_AUTHOR_NAME", "Hivemind")
                .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                .env("GIT_COMMITTER_NAME", "Hivemind")
                .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
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
                .map_err(|e| HivemindError::system("git_merge_failed", e.to_string(), origin))?;
            if !merge.status.success() {
                let unmerged = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["diff", "--name-only", "--diff-filter=U"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();

                let details = if unmerged.is_empty() {
                    String::from_utf8_lossy(&merge.stderr).to_string()
                } else {
                    format!("conflicts in: {unmerged}")
                };
                conflicts.push(format!("task {task_id}: {details}"));
                self.emit_merge_conflict(flow, Some(task_id), details, origin)?;

                let _ = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["merge", "--abort"])
                    .output();
                let _ = Command::new("git")
                    .current_dir(&merge_path)
                    .args(["checkout", &merge_branch])
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
                continue;
            }

            let commit_msg = format!(
                "Integrate task {task_id}\n\nFlow: {}\nTask: {task_id}\nTarget: {prepared_target_branch}\nVerification-Summary: task_checks_passed\nTimestamp: {}\nHivemind-Version: {}",
                flow.id,
                Utc::now().to_rfc3339(),
                env!("CARGO_PKG_VERSION")
            );
            let commit = Command::new("git")
                .current_dir(&merge_path)
                .env("GIT_AUTHOR_NAME", "Hivemind")
                .env("GIT_AUTHOR_EMAIL", "hivemind@example.com")
                .env("GIT_COMMITTER_NAME", "Hivemind")
                .env("GIT_COMMITTER_EMAIL", "hivemind@example.com")
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
                .map_err(|e| HivemindError::system("git_commit_failed", e.to_string(), origin))?;
            if !commit.status.success() {
                return Err(HivemindError::git(
                    "git_commit_failed",
                    String::from_utf8_lossy(&commit.stderr).to_string(),
                    origin,
                ));
            }

            let _ = Command::new("git")
                .current_dir(&merge_path)
                .args(["checkout", &merge_branch])
                .output();
            let promote = Command::new("git")
                .current_dir(&merge_path)
                .args(["merge", "--ff-only", &sandbox_branch])
                .output()
                .map_err(|e| HivemindError::system("git_merge_failed", e.to_string(), origin))?;
            if !promote.status.success() {
                let details = String::from_utf8_lossy(&promote.stderr).trim().to_string();
                conflicts.push(format!("task {task_id}: {details}"));
                self.emit_merge_conflict(flow, Some(task_id), details, origin)?;
                break;
            }

            let integrated_sha = Self::resolve_git_ref(&merge_path, "HEAD");
            integrated_tasks.push((task_id, integrated_sha));
        }

        Ok(PrimaryPrepareOutcome {
            merge_branch,
            merge_path,
            integrated_tasks,
            conflicts,
        })
    }

    pub(super) fn publish_primary_prepare_branch(
        manager: &WorktreeManager,
        flow_id: Uuid,
        merge_branch: &str,
        origin: &'static str,
    ) -> Result<()> {
        let flow_branch = format!("flow/{flow_id}");
        let update = Command::new("git")
            .current_dir(manager.repo_path())
            .args(["branch", "-f", &flow_branch, merge_branch])
            .output()
            .map_err(|e| {
                HivemindError::system("git_branch_update_failed", e.to_string(), origin)
            })?;
        if !update.status.success() {
            return Err(HivemindError::git(
                "git_branch_update_failed",
                String::from_utf8_lossy(&update.stderr).to_string(),
                origin,
            ));
        }
        Ok(())
    }
}
