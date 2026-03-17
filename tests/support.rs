#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

pub(crate) fn init_git_repo(repo_dir: &std::path::Path) {
    std::fs::create_dir_all(repo_dir).expect("create repo dir");

    let out = Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .expect("git init");
    assert!(
        out.status.success(),
        "git init: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    std::fs::write(repo_dir.join("README.md"), "test\n").expect("write file");

    let out = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_dir)
        .output()
        .expect("git add");
    assert!(
        out.status.success(),
        "git add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Hivemind",
            "-c",
            "user.email=hivemind@example.com",
            "commit",
            "-m",
            "init",
        ])
        .current_dir(repo_dir)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(crate) fn git_commit_all(repo_dir: &std::path::Path, message: &str) {
    let out = Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_dir)
        .output()
        .expect("git add");
    assert!(
        out.status.success(),
        "git add: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Hivemind",
            "-c",
            "user.email=hivemind@example.com",
            "commit",
            "-m",
            message,
        ])
        .current_dir(repo_dir)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(crate) fn hivemind_bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_hivemind").map_or_else(
        || {
            std::env::var("CARGO_BIN_EXE_hivemind")
                .map(PathBuf::from)
                .expect("CARGO_BIN_EXE_hivemind not set; build the hivemind binary")
        },
        PathBuf::from,
    )
}

pub(crate) fn run_hivemind(home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(hivemind_bin())
        .env("HOME", home)
        .args(args)
        .output()
        .expect("run hivemind");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

pub(crate) fn worktree_root(home: &std::path::Path) -> PathBuf {
    home.join("hivemind").join("worktrees")
}

pub(crate) fn run_hivemind_with_env(
    home: &std::path::Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new(hivemind_bin());
    cmd.env("HOME", home).args(args);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("run hivemind");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}
