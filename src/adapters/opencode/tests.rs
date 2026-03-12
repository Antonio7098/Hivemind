use super::*;
use crate::adapters::codex::{CodexAdapter, CodexConfig};
use crate::adapters::runtime::StructuredRuntimeObservation;
use crate::core::events::RuntimeOutputStream;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

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
        native_prompt_metadata: None,
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
        native_prompt_metadata: None,
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
        native_prompt_metadata: None,
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
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then exit 1; fi\nif [ \"$1\" = \"--help\" ]; then exit 0; fi\nexit 0\n",
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
        native_prompt_metadata: None,
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
        native_prompt_metadata: None,
    };

    let report = adapter.execute(input).unwrap();
    assert_eq!(report.exit_code, 7);
    assert!(report.errors.iter().any(|e| e.code == "nonzero_exit"));
}

fn write_executable(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[test]
fn execute_opencode_binary_forces_json_mode_and_projects_events() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let binary_path = tmp.path().join("opencode");
    let arg_log = tmp.path().join("opencode.args");

    write_executable(
        &binary_path,
        r#"#!/bin/sh
set -eu
if [ "${1-}" = "--version" ] || [ "${1-}" = "--help" ]; then
  exit 0
fi
printf '%s\n' "$@" > "$ARG_LOG"
printf '%s\n' '{"type":"step_start"}'
printf '%s\n' '{"type":"tool_use","part":{"tool":"bash","state":{"input":{"command":"pwd"},"output":"/tmp/runtime\n","title":"Ran pwd"}}}'
printf '%s\n' '{"type":"text","part":{"text":"done"}}'
printf '%s\n' '{"type":"step_finish","part":{"reason":"stop"}}'
"#,
    );

    let mut cfg = OpenCodeConfig::new(binary_path);
    cfg.base.timeout = Duration::from_secs(1);
    cfg.base
        .env
        .insert("ARG_LOG".to_string(), arg_log.display().to_string());

    let mut adapter = OpenCodeAdapter::new(cfg);
    adapter.initialize().unwrap();
    adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

    let report = adapter
        .execute(ExecutionInput {
            task_description: "Test JSON output".to_string(),
            success_criteria: "done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
            native_prompt_metadata: None,
        })
        .unwrap();

    let logged_args = std::fs::read_to_string(&arg_log).unwrap();
    assert!(logged_args.contains("run\n"));
    assert!(logged_args.contains("--format\njson\n"));
    assert!(report.stdout.contains("Tool: bash"));
    assert!(report.stdout.contains("Command: pwd"));
    assert!(report.stdout.contains("Ran pwd"));
    assert!(report.stdout.contains("done"));
    assert!(report.stderr.contains("[opencode.json]"));
    assert_eq!(
        report.structured_runtime_observations,
        vec![
            StructuredRuntimeObservation::TurnCompleted {
                stream: RuntimeOutputStream::Stdout,
                adapter_name: "opencode".to_string(),
                ordinal: 1,
                provider_session_id: None,
                provider_turn_id: None,
                git_ref: None,
                commit_sha: None,
                summary: None,
            },
            StructuredRuntimeObservation::CommandCompleted {
                stream: RuntimeOutputStream::Stdout,
                command: "pwd".to_string(),
                exit_code: None,
                output: Some("/tmp/runtime\n".to_string()),
            },
        ]
    );
}

#[test]
fn execute_codex_binary_forces_json_mode_and_projects_events() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let binary_path = tmp.path().join("codex");
    let arg_log = tmp.path().join("codex.args");
    let stdin_log = tmp.path().join("codex.stdin");

    write_executable(
        &binary_path,
        r#"#!/bin/sh
set -eu
if [ "${1-}" = "--version" ] || [ "${1-}" = "--help" ]; then
  exit 0
fi
printf '%s\n' "$@" > "$ARG_LOG"
cat > "$STDIN_LOG"
printf '%s\n' '{"type":"thread.started","thread_id":"thread-1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.started","item":{"id":"item-1","type":"command_execution","command":"/bin/bash -lc pwd","aggregated_output":"","exit_code":null,"status":"in_progress"}}'
printf '%s\n' '{"type":"item.completed","item":{"id":"item-1","type":"command_execution","command":"/bin/bash -lc pwd","aggregated_output":"/tmp/runtime\n","exit_code":0,"status":"completed"}}'
printf '%s\n' '{"type":"item.completed","item":{"id":"item-2","type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"input_tokens":7,"cached_input_tokens":0,"output_tokens":3}}'
"#,
    );

    let mut cfg = CodexConfig::new(binary_path);
    cfg.base.args = CodexConfig::default().base.args;
    cfg.base.timeout = Duration::from_secs(1);
    cfg.base
        .env
        .insert("ARG_LOG".to_string(), arg_log.display().to_string());
    cfg.base
        .env
        .insert("STDIN_LOG".to_string(), stdin_log.display().to_string());

    let mut adapter = CodexAdapter::new(cfg);
    adapter.initialize().unwrap();
    adapter.prepare(Uuid::new_v4(), tmp.path()).unwrap();

    let report = adapter
        .execute(ExecutionInput {
            task_description: "Test Codex JSON output".to_string(),
            success_criteria: "done".to_string(),
            context: Some("verify stdin delivery".to_string()),
            prior_attempts: Vec::new(),
            verifier_feedback: None,
            native_prompt_metadata: None,
        })
        .unwrap();

    let logged_args = std::fs::read_to_string(&arg_log).unwrap();
    let logged_stdin = std::fs::read_to_string(&stdin_log).unwrap();
    assert!(logged_args.contains("exec\n"));
    assert!(logged_args.contains("--json\n"));
    assert!(logged_args.contains("--skip-git-repo-check\n"));
    assert!(logged_stdin.contains("Test Codex JSON output"));
    assert!(report.stdout.contains("Thread started: thread-1"));
    assert!(report.stdout.contains("Command: /bin/bash -lc pwd"));
    assert!(report.stdout.contains("/tmp/runtime"));
    assert!(report.stdout.contains("done"));
    assert!(report.stderr.contains("[codex.json]"));
    assert_eq!(
        report.structured_runtime_observations,
        vec![
            StructuredRuntimeObservation::SessionObserved {
                stream: RuntimeOutputStream::Stdout,
                adapter_name: "codex".to_string(),
                session_id: "thread-1".to_string(),
            },
            StructuredRuntimeObservation::TurnCompleted {
                stream: RuntimeOutputStream::Stdout,
                adapter_name: "codex".to_string(),
                ordinal: 1,
                provider_session_id: Some("thread-1".to_string()),
                provider_turn_id: None,
                git_ref: None,
                commit_sha: None,
                summary: None,
            },
            StructuredRuntimeObservation::CommandCompleted {
                stream: RuntimeOutputStream::Stdout,
                command: "/bin/bash -lc pwd".to_string(),
                exit_code: Some(0),
                output: Some("/tmp/runtime\n".to_string()),
            },
        ]
    );
}
