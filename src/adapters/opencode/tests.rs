use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn opencode_config_creation() {
    let config = OpenCodeConfig::new(PathBuf::from("/usr/bin/opencode"))
        .with_model("gpt-4")
        .with_verbose(true)
        .with_timeout(Duration::from_secs(120));

    assert_eq!(config.model, Some("gpt-4".to_string()));
    assert!(config.verbose);
    assert_eq!(config.base.timeout, Duration::from_secs(120));
}

#[test]
fn opencode_config_default() {
    let config = OpenCodeConfig::default();
    assert_eq!(config.base.name, "opencode");
    assert!(config.model.is_none());
    assert!(!config.verbose);
}

#[test]
fn adapter_creation() {
    let adapter = OpenCodeAdapter::with_defaults();
    assert_eq!(adapter.name(), "opencode");
}

#[test]
fn input_formatting() {
    let adapter = OpenCodeAdapter::with_defaults();

    let input = ExecutionInput {
        task_description: "Write a function".to_string(),
        success_criteria: "Function works".to_string(),
        context: Some("This is for testing".to_string()),
        prior_attempts: vec![],
        verifier_feedback: None,
    };

    let formatted = adapter.format_input(&input);
    assert!(formatted.contains("Write a function"));
    assert!(formatted.contains("Function works"));
    assert!(formatted.contains("This is for testing"));
}

#[test]
fn input_formatting_with_retries() {
    let adapter = OpenCodeAdapter::with_defaults();

    let input = ExecutionInput {
        task_description: "Fix the bug".to_string(),
        success_criteria: "Tests pass".to_string(),
        context: None,
        prior_attempts: vec![super::super::runtime::AttemptSummary {
            attempt_number: 1,
            summary: "Tried approach A".to_string(),
            failure_reason: Some("Tests failed".to_string()),
        }],
        verifier_feedback: Some("Check edge cases".to_string()),
    };

    let formatted = adapter.format_input(&input);
    assert!(formatted.contains("Attempt 1"));
    assert!(formatted.contains("Tried approach A"));
    assert!(formatted.contains("Check edge cases"));
}

#[test]
fn prepare_requires_existing_worktree() {
    let mut adapter = OpenCodeAdapter::with_defaults();
    let task_id = Uuid::new_v4();

    let result = adapter.prepare(task_id, &PathBuf::from("/nonexistent/path"));
    assert!(result.is_err());
}

#[test]
fn prepare_with_valid_worktree() {
    let mut adapter = OpenCodeAdapter::with_defaults();
    let task_id = Uuid::new_v4();

    let result = adapter.prepare(task_id, &PathBuf::from("/tmp"));
    assert!(result.is_ok());
    assert!(adapter.worktree.is_some());
    assert!(adapter.task_id.is_some());
}

#[test]
fn terminate_clears_state() {
    let mut adapter = OpenCodeAdapter::with_defaults();
    adapter.worktree = Some(PathBuf::from("/tmp"));
    adapter.task_id = Some(Uuid::new_v4());

    adapter.terminate().unwrap();

    assert!(adapter.worktree.is_none());
    assert!(adapter.task_id.is_none());
}

#[test]
fn execute_enforces_timeout() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
    cfg.base.args = vec!["sh".to_string(), "-c".to_string(), "sleep 2".to_string()];
    cfg.base.timeout = Duration::from_millis(50);

    let mut adapter = OpenCodeAdapter::new(cfg);
    adapter.initialize().unwrap();
    adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

    let input = ExecutionInput {
        task_description: "Test".to_string(),
        success_criteria: "Done".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
    };

    let err = adapter.execute(input).unwrap_err();
    assert_eq!(err.code, "timeout");
}

#[test]
fn initialize_falls_back_to_help_when_version_fails() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let script_path = tmp.path().join("fake_runtime.sh");
    std::fs::write(
        &script_path,
        "#!/usr/bin/env sh\nif [ \"$1\" = \"--version\" ]; then exit 1; fi\nif [ \"$1\" = \"--help\" ]; then exit 0; fi\nexit 0\n",
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script_path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms).unwrap();

    let cfg = OpenCodeConfig::new(script_path);
    let mut adapter = OpenCodeAdapter::new(cfg);
    adapter.initialize().unwrap();
}

#[test]
fn execute_success_captures_stdout_and_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
    cfg.base.args = vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo ok_stdout; echo ok_stderr 1>&2".to_string(),
    ];
    cfg.base.timeout = Duration::from_secs(1);

    let mut adapter = OpenCodeAdapter::new(cfg);
    adapter.initialize().unwrap();
    adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

    let input = ExecutionInput {
        task_description: "Test".to_string(),
        success_criteria: "Done".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
    };

    let report = adapter.execute(input).unwrap();
    assert_eq!(report.exit_code, 0);
    assert!(report.stdout.contains("ok_stdout"));
    assert!(report.stderr.contains("ok_stderr"));
}

#[test]
fn execute_nonzero_exit_returns_failure_report() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let mut cfg = OpenCodeConfig::new(PathBuf::from("/usr/bin/env"));
    cfg.base.args = vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo bad; exit 7".to_string(),
    ];
    cfg.base.timeout = Duration::from_secs(1);

    let mut adapter = OpenCodeAdapter::new(cfg);
    adapter.initialize().unwrap();
    adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

    let input = ExecutionInput {
        task_description: "Test".to_string(),
        success_criteria: "Done".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
    };

    let report = adapter.execute(input).unwrap();
    assert_eq!(report.exit_code, 7);
    assert!(report.errors.iter().any(|e| e.code == "nonzero_exit"));
}
