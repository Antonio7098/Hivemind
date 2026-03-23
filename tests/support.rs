#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Command;

pub fn init_git_repo(repo_dir: &std::path::Path) {
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

pub fn git_commit_all(repo_dir: &std::path::Path, message: &str) {
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

pub fn hivemind_bin() -> PathBuf {
    option_env!("CARGO_BIN_EXE_hivemind").map_or_else(
        || {
            std::env::var("CARGO_BIN_EXE_hivemind")
                .map(PathBuf::from)
                .expect("CARGO_BIN_EXE_hivemind not set; build the hivemind binary")
        },
        PathBuf::from,
    )
}

pub fn run_hivemind(home: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let data_dir = home.join(".hivemind");
    let worktree_dir = home.join("hivemind").join("worktrees");
    let output = Command::new(hivemind_bin())
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("HIVEMIND_DATA_DIR", &data_dir)
        .env("HIVEMIND_WORKTREE_DIR", &worktree_dir)
        .args(args)
        .output()
        .expect("run hivemind");

    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

pub fn set_project_runtime_script(
    home: &std::path::Path,
    project: &str,
    unix_script: &str,
    windows_script: &str,
    timeout_ms: u64,
) -> (i32, String, String) {
    set_project_runtime_script_with_model(
        home,
        project,
        None,
        unix_script,
        windows_script,
        timeout_ms,
    )
}

pub fn set_project_native_scripted_runtime(
    home: &std::path::Path,
    project: &str,
    directives: &[&str],
    timeout_ms: u64,
) -> (i32, String, String) {
    let directives_json = serde_json::to_string(directives).expect("encode scripted directives");
    let timeout_ms = if cfg!(windows) {
        timeout_ms.max(5_000)
    } else {
        timeout_ms
    };
    let directives_env = format!("HIVEMIND_NATIVE_SCRIPTED_DIRECTIVES_JSON={directives_json}");
    let timeout_arg = timeout_ms.to_string();
    let args = vec![
        "project".to_string(),
        "runtime-set".to_string(),
        project.to_string(),
        "--adapter".to_string(),
        "native".to_string(),
        "--binary-path".to_string(),
        "builtin-native".to_string(),
        "--model".to_string(),
        "mock-model".to_string(),
        "--env".to_string(),
        "HIVEMIND_NATIVE_PROVIDER=mock".to_string(),
        "--env".to_string(),
        directives_env,
        "--timeout-ms".to_string(),
        timeout_arg,
    ];
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_hivemind(home, &arg_refs)
}

pub fn set_project_runtime_script_with_model(
    home: &std::path::Path,
    project: &str,
    model: Option<&str>,
    unix_script: &str,
    windows_script: &str,
    timeout_ms: u64,
) -> (i32, String, String) {
    let timeout_ms = if cfg!(windows) {
        timeout_ms.max(5_000)
    } else {
        timeout_ms
    };
    let windows_script = if cfg!(windows)
        && windows_script.contains("checkpoint complete --id")
        && !windows_script.contains("--attempt-id")
    {
        windows_script.replace(
            "checkpoint complete --id",
            "checkpoint complete --attempt-id %HIVEMIND_ATTEMPT_ID% --id",
        )
    } else {
        windows_script.to_string()
    };
    let windows_script = if cfg!(windows) && !windows_script.contains("exit /b") {
        format!("{windows_script} & exit /b 0")
    } else {
        windows_script
    };

    let mut args = vec![
        "project".to_string(),
        "runtime-set".to_string(),
        project.to_string(),
        "--binary-path".to_string(),
    ];

    if cfg!(windows) {
        args.push("cmd.exe".to_string());
    } else {
        args.push("/usr/bin/env".to_string());
    }

    if let Some(model) = model {
        args.push("--model".to_string());
        args.push(model.to_string());
    }

    args.push("--arg".to_string());

    if cfg!(windows) {
        args.push("/C".to_string());
        args.push("--arg".to_string());
        args.push(windows_script);
    } else {
        args.push("sh".to_string());
        args.push("--arg".to_string());
        args.push("-c".to_string());
        args.push("--arg".to_string());
        args.push(unix_script.to_string());
    }

    args.push("--timeout-ms".to_string());
    args.push(timeout_ms.to_string());

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_hivemind(home, &arg_refs)
}

pub fn expected_runtime_flag_prefix() -> &'static [&'static str] {
    if cfg!(windows) {
        &["/C"]
    } else {
        &["sh", "-c"]
    }
}

pub fn failing_check_command() -> &'static str {
    if cfg!(windows) {
        "exit /b 1"
    } else {
        "exit 1"
    }
}

pub fn worktree_root(home: &std::path::Path) -> PathBuf {
    home.join("hivemind").join("worktrees")
}

pub fn run_hivemind_with_env(
    home: &std::path::Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new(hivemind_bin());
    let data_dir = home.join(".hivemind");
    let worktree_dir = home.join("hivemind").join("worktrees");
    cmd.env("HOME", home)
        .env("USERPROFILE", home)
        .env("HIVEMIND_DATA_DIR", &data_dir)
        .env("HIVEMIND_WORKTREE_DIR", &worktree_dir)
        .args(args);
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
