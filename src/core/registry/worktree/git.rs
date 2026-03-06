use super::*;

impl Registry {
    pub(crate) fn detect_git_operations(
        worktree_path: &Path,
        baseline: &Baseline,
        attempt_id: Uuid,
    ) -> (bool, bool) {
        let commits_created = Self::detect_commits_created(worktree_path, baseline, attempt_id);
        let branches_created = Self::detect_branches_created(worktree_path, baseline);
        (commits_created, branches_created)
    }

    pub(crate) fn detect_commits_created(
        worktree_path: &Path,
        baseline: &Baseline,
        attempt_id: Uuid,
    ) -> bool {
        let Some(base) = baseline.git_head.as_deref() else {
            return false;
        };

        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["log", "--format=%s", &format!("{base}..HEAD")])
            .output();

        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let mut subjects: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        subjects.retain(|s| s != &format!("hivemind checkpoint {attempt_id}"));
        subjects.retain(|s| !s.starts_with("hivemind(checkpoint): "));
        !subjects.is_empty()
    }

    pub(crate) fn detect_branches_created(worktree_path: &Path, baseline: &Baseline) -> bool {
        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["for-each-ref", "refs/heads", "--format=%(refname:short)"])
            .output();

        let Ok(output) = output else {
            return false;
        };
        if !output.status.success() {
            return false;
        }

        let current: std::collections::HashSet<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        let base: std::collections::HashSet<String> =
            baseline.git_branches.iter().cloned().collect();
        current.difference(&base).next().is_some()
    }

    pub(crate) fn parse_git_status_paths(worktree_path: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .current_dir(worktree_path)
            .args(["status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                line.strip_prefix("?? ")
                    .or_else(|| line.get(3..))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect()
    }

    pub(crate) fn repo_git_head(path: &Path) -> Option<String> {
        std::process::Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|head| !head.is_empty())
    }

    pub(crate) fn repo_status_lines(path: &Path) -> Vec<String> {
        let output = std::process::Command::new("git")
            .current_dir(path)
            .args(["status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        lines.retain(|line| {
            let path = line
                .strip_prefix("?? ")
                .or_else(|| line.get(3..))
                .unwrap_or("")
                .trim();
            !path.starts_with(".hivemind/")
        });
        lines.sort();
        lines.dedup();
        lines
    }
}
