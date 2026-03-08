use super::*;

impl WorktreeManager {
    pub fn new(repo_path: PathBuf, config: WorktreeConfig) -> Result<Self> {
        let WorktreeConfig {
            base_dir,
            cleanup_on_success,
            preserve_on_failure,
        } = config;
        let is_git_repo = Command::new("git")
            .current_dir(&repo_path)
            .args(["rev-parse", "--git-dir"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !is_git_repo {
            return Err(WorktreeError::InvalidRepo(repo_path));
        }

        let base_dir = if base_dir.is_absolute() {
            base_dir
        } else {
            repo_path.join(base_dir)
        };

        Ok(Self {
            repo_path,
            config: WorktreeConfig {
                base_dir,
                cleanup_on_success,
                preserve_on_failure,
            },
        })
    }

    pub fn create(
        &self,
        flow_id: Uuid,
        task_id: Uuid,
        base_ref: Option<&str>,
    ) -> Result<WorktreeInfo> {
        let worktree_id = Uuid::new_v4();
        let branch_name = format!("exec/{flow_id}/{task_id}");
        let worktree_path = self
            .config
            .base_dir
            .join(flow_id.to_string())
            .join(task_id.to_string());

        if worktree_path.exists() {
            return Err(WorktreeError::AlreadyExists(task_id));
        }
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let base = base_ref.unwrap_or("HEAD");
        let base_commit = self.get_commit_hash(base)?;
        let worktree_path_str = worktree_path.to_str().ok_or_else(|| {
            WorktreeError::GitError("Worktree path is not valid UTF-8".to_string())
        })?;
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args([
                "worktree",
                "add",
                "-B",
                &branch_name,
                worktree_path_str,
                base,
            ])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(WorktreeInfo {
            id: worktree_id,
            task_id,
            flow_id,
            path: worktree_path,
            branch: branch_name,
            base_commit,
        })
    }

    pub fn remove(&self, worktree_path: &Path) -> Result<()> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args([
                "worktree",
                "remove",
                "--force",
                worktree_path.to_str().unwrap_or(""),
            ])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(())
    }

    pub fn is_worktree(&self, path: &Path) -> bool {
        if !path.join(".git").exists() {
            return false;
        }
        match Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .trim()
                .eq_ignore_ascii_case("true"),
            _ => false,
        }
    }

    pub fn worktree_head(&self, worktree_path: &Path) -> Result<String> {
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub fn commit(&self, worktree_path: &Path, message: &str) -> Result<String> {
        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["add", "-A"])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let output = Command::new("git")
            .current_dir(worktree_path)
            .args(["commit", "-m", message, "--allow-empty"])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        self.worktree_head(worktree_path)
    }

    pub fn create_hidden_snapshot_ref(
        &self,
        worktree_path: &Path,
        ref_name: &str,
        message: &str,
    ) -> Result<String> {
        let temp_index = std::env::temp_dir().join(format!(
            "hivemind-turn-ref-{}-{}.index",
            std::process::id(),
            Uuid::new_v4()
        ));

        let cleanup = |path: &Path| {
            let _ = std::fs::remove_file(path);
        };

        let head = match Command::new("git")
            .current_dir(worktree_path)
            .args(["rev-parse", "HEAD"])
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                cleanup(&temp_index);
                return Err(WorktreeError::GitError(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
            Err(error) => {
                cleanup(&temp_index);
                return Err(WorktreeError::IoError(error));
            }
        };

        let with_index = |args: &[&str]| -> Result<std::process::Output> {
            Command::new("git")
                .current_dir(worktree_path)
                .env("GIT_INDEX_FILE", &temp_index)
                .args(args)
                .output()
                .map_err(WorktreeError::IoError)
        };

        let read_tree = with_index(&["read-tree", "HEAD"])?;
        if !read_tree.status.success() {
            cleanup(&temp_index);
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&read_tree.stderr).to_string(),
            ));
        }

        let add = with_index(&["add", "-A"])?;
        if !add.status.success() {
            cleanup(&temp_index);
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&add.stderr).to_string(),
            ));
        }

        let tree = with_index(&["write-tree"])?;
        if !tree.status.success() {
            cleanup(&temp_index);
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&tree.stderr).to_string(),
            ));
        }
        let tree_sha = String::from_utf8_lossy(&tree.stdout).trim().to_string();

        let commit = Command::new("git")
            .current_dir(worktree_path)
            .args([
                "-c",
                "user.name=Hivemind",
                "-c",
                "user.email=hivemind@example.com",
                "commit-tree",
                &tree_sha,
                "-p",
                &head,
                "-m",
                message,
            ])
            .output()?;
        if !commit.status.success() {
            cleanup(&temp_index);
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&commit.stderr).to_string(),
            ));
        }
        let commit_sha = String::from_utf8_lossy(&commit.stdout).trim().to_string();

        let update_ref = Command::new("git")
            .current_dir(worktree_path)
            .args(["update-ref", ref_name, &commit_sha])
            .output()?;
        cleanup(&temp_index);
        if !update_ref.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&update_ref.stderr).to_string(),
            ));
        }

        Ok(commit_sha)
    }

    pub fn restore_hidden_snapshot_ref(&self, worktree_path: &Path, reference: &str) -> Result<()> {
        let restore = Command::new("git")
            .current_dir(worktree_path)
            .args([
                "restore",
                "--source",
                reference,
                "--staged",
                "--worktree",
                ":/",
            ])
            .output()?;
        if !restore.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&restore.stderr).to_string(),
            ));
        }

        let clean = Command::new("git")
            .current_dir(worktree_path)
            .args(["clean", "-fd", "-e", ".hivemind/"])
            .output()?;
        if !clean.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&clean.stderr).to_string(),
            ));
        }

        Ok(())
    }

    pub fn has_uncommitted_changes(&self, worktree_path: &Path) -> Result<bool> {
        let status = Command::new("git")
            .current_dir(worktree_path)
            .args(["status", "--porcelain", "--untracked-files=all"])
            .output()?;
        if !status.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&status.stderr).to_string(),
            ));
        }

        Ok(!String::from_utf8_lossy(&status.stdout).trim().is_empty())
    }

    fn get_commit_hash(&self, reference: &str) -> Result<String> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["rev-parse", reference])
            .output()?;
        if !output.status.success() {
            return Err(WorktreeError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
