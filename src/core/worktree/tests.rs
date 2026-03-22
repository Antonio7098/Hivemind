use super::*;
use tempfile::tempdir;

fn init_git_repo(repo_dir: &Path) {
    std::fs::create_dir_all(repo_dir).expect("create repo dir");

    let out = Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .expect("git init");
    assert!(out.status.success(), "git init failed");

    let out = Command::new("git")
        .args(["config", "user.name", "Hivemind"])
        .current_dir(repo_dir)
        .output()
        .expect("git config user.name");
    assert!(out.status.success(), "git config user.name failed");

    let out = Command::new("git")
        .args(["config", "user.email", "hivemind@example.com"])
        .current_dir(repo_dir)
        .output()
        .expect("git config user.email");
    assert!(out.status.success(), "git config user.email failed");

    std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");
    let out = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_dir)
        .output()
        .expect("git add");
    assert!(out.status.success(), "git add failed");
    let out = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_dir)
        .output()
        .expect("git commit");
    assert!(out.status.success(), "git commit failed");
}

#[test]
fn worktree_config_default() {
    let config = WorktreeConfig::default();
    assert!(config.cleanup_on_success);
    assert!(config.preserve_on_failure);
}

#[test]
fn worktree_info_creation() {
    let info = WorktreeInfo {
        id: Uuid::new_v4(),
        task_id: Uuid::new_v4(),
        flow_id: Uuid::new_v4(),
        workflow_id: None,
        workflow_run_id: None,
        step_id: None,
        step_run_id: None,
        path: PathBuf::from("/tmp/test"),
        branch: "test-branch".to_string(),
        base_commit: "abc123".to_string(),
    };
    assert!(!info.branch.is_empty());
}

#[test]
fn invalid_repo_detection() {
    let result = WorktreeManager::new(
        PathBuf::from("/nonexistent/path"),
        WorktreeConfig::default(),
    );
    assert!(result.is_err());
}

#[test]
fn create_inspect_list_commit_and_cleanup() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let manager = WorktreeManager::new(
        repo_dir,
        WorktreeConfig {
            base_dir: tmp.path().join("worktrees"),
            cleanup_on_success: true,
            preserve_on_failure: true,
        },
    )
    .expect("worktree manager");

    let flow_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let info = manager
        .create(flow_id, task_id, None)
        .expect("create worktree");
    assert!(info.path.exists());
    assert!(manager.is_worktree(&info.path));

    let status = manager.inspect(flow_id, task_id).expect("inspect");
    assert!(status.is_worktree);
    assert_eq!(status.flow_id, flow_id);
    assert_eq!(status.task_id, task_id);
    assert!(status.head_commit.is_some());

    let listed = manager.list_for_flow(flow_id).expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], info.path);

    std::fs::write(info.path.join("file.txt"), "hello\n").expect("write file");
    let head = manager.commit(&info.path, "commit").expect("commit");
    assert!(!head.trim().is_empty());

    manager.cleanup_flow(flow_id).expect("cleanup");
    assert!(!info.path.exists());
}

#[test]
fn create_hidden_snapshot_ref_captures_worktree_state_without_moving_head() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let manager = WorktreeManager::new(
        repo_dir,
        WorktreeConfig {
            base_dir: tmp.path().join("worktrees"),
            cleanup_on_success: true,
            preserve_on_failure: true,
        },
    )
    .expect("worktree manager");

    let info = manager
        .create(Uuid::new_v4(), Uuid::new_v4(), None)
        .expect("create worktree");

    let head_before = manager.worktree_head(&info.path).expect("head before");
    std::fs::write(info.path.join("turn.txt"), "atomic\n").expect("write turn file");

    let ref_name = "refs/hivemind/transient/turns/test/0001";
    let commit_sha = manager
        .create_hidden_snapshot_ref(&info.path, ref_name, "turn snapshot")
        .expect("create turn ref");

    let head_after = manager.worktree_head(&info.path).expect("head after");
    assert_eq!(head_before, head_after);

    let resolved_ref = Command::new("git")
        .current_dir(&info.path)
        .args(["rev-parse", ref_name])
        .output()
        .expect("resolve ref");
    assert!(resolved_ref.status.success(), "resolve ref failed");
    assert_eq!(
        String::from_utf8_lossy(&resolved_ref.stdout).trim(),
        commit_sha.as_str()
    );

    let show = Command::new("git")
        .current_dir(&info.path)
        .args(["show", &format!("{ref_name}:turn.txt")])
        .output()
        .expect("show turn file");
    assert!(show.status.success(), "show turn file failed");
    assert_eq!(String::from_utf8_lossy(&show.stdout), "atomic\n");
}

#[test]
fn restore_hidden_snapshot_ref_restores_snapshot_without_moving_head() {
    let tmp = tempdir().expect("tempdir");
    let repo_dir = tmp.path().join("repo");
    init_git_repo(&repo_dir);

    let manager = WorktreeManager::new(
        repo_dir,
        WorktreeConfig {
            base_dir: tmp.path().join("worktrees"),
            cleanup_on_success: true,
            preserve_on_failure: true,
        },
    )
    .expect("worktree manager");

    let info = manager
        .create(Uuid::new_v4(), Uuid::new_v4(), None)
        .expect("create worktree");

    let head_before = manager.worktree_head(&info.path).expect("head before");
    std::fs::write(info.path.join("README.md"), "snapshot\n").expect("write readme");
    std::fs::write(info.path.join("turn.txt"), "turn one\n").expect("write turn file");
    let ref_name = "refs/hivemind/transient/turns/test/restore/0001";
    manager
        .create_hidden_snapshot_ref(&info.path, ref_name, "turn snapshot")
        .expect("create turn ref");

    std::fs::write(info.path.join("README.md"), "after\n").expect("write readme after");
    std::fs::remove_file(info.path.join("turn.txt")).expect("remove turn file");
    std::fs::write(info.path.join("extra.txt"), "should disappear\n").expect("write extra file");
    assert!(manager
        .has_uncommitted_changes(&info.path)
        .expect("dirty status before restore"));

    manager
        .restore_hidden_snapshot_ref(&info.path, ref_name)
        .expect("restore hidden snapshot ref");

    let head_after = manager.worktree_head(&info.path).expect("head after");
    assert_eq!(head_before, head_after);
    assert_eq!(
        std::fs::read_to_string(info.path.join("README.md")).expect("read restored readme"),
        "snapshot\n"
    );
    assert_eq!(
        std::fs::read_to_string(info.path.join("turn.txt")).expect("read restored turn file"),
        "turn one\n"
    );
    assert!(!info.path.join("extra.txt").exists());
    assert!(manager
        .has_uncommitted_changes(&info.path)
        .expect("dirty status after restore"));
}
