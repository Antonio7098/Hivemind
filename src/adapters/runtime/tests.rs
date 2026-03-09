use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn execution_input_creation() {
    let input = ExecutionInput {
        task_description: "Write a test".to_string(),
        success_criteria: "Test passes".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
        native_prompt_metadata: None,
    };

    assert!(input.prior_attempts.is_empty());
}

#[test]
fn execution_report_success() {
    let report =
        ExecutionReport::success(Duration::from_secs(5), "output".to_string(), String::new());
    assert!(report.is_success());
    assert_eq!(report.exit_code, 0);
}

#[test]
fn execution_report_failure() {
    let report = ExecutionReport::failure(
        1,
        Duration::from_secs(2),
        RuntimeError::new("test_error", "Test failed", false),
    );
    assert!(!report.is_success());
    assert_eq!(report.exit_code, 1);
}

#[test]
fn adapter_config_builder() {
    let config = AdapterConfig::new("test", PathBuf::from("/bin/test"))
        .with_timeout(Duration::from_secs(60))
        .with_arg("--verbose")
        .with_env("DEBUG", "true");

    assert_eq!(config.name, "test");
    assert_eq!(config.timeout, Duration::from_secs(60));
    assert_eq!(config.args, vec!["--verbose"]);
    assert_eq!(config.env.get("DEBUG"), Some(&"true".to_string()));
}

#[test]
fn parse_env_inherit_mode_rejects_invalid_values() {
    let mut overlay = HashMap::new();
    overlay.insert(
        env::ENV_POLICY_INHERIT_MODE_KEY_FOR_TEST.to_string(),
        "mystery".to_string(),
    );
    let error =
        env::parse_env_inherit_mode_for_test(&overlay).expect_err("invalid mode should fail");
    assert_eq!(error.code, "runtime_env_inherit_mode_invalid");
}

#[test]
fn protected_runtime_env_none_mode_uses_only_overlay() {
    let mut overlay = HashMap::new();
    overlay.insert(
        env::ENV_POLICY_INHERIT_MODE_KEY_FOR_TEST.to_string(),
        env::ENV_POLICY_INHERIT_NONE_FOR_TEST.to_string(),
    );
    overlay.insert("PATH".to_string(), "/overlay/bin".to_string());
    overlay.insert("OPENROUTER_API_KEY".to_string(), "secret".to_string());

    let built = build_protected_runtime_environment(&overlay).expect("env build should work");
    assert_eq!(built.provenance.inherit_mode, RuntimeEnvInheritMode::None);
    assert!(built.provenance.inherited_keys.is_empty());
    assert_eq!(built.env.get("PATH"), Some(&"/overlay/bin".to_string()));
    assert_eq!(
        built.provenance.explicit_sensitive_overlay_keys,
        vec!["OPENROUTER_API_KEY".to_string()]
    );
    assert!(!built
        .env
        .contains_key(env::ENV_POLICY_INHERIT_MODE_KEY_FOR_TEST));
}

#[test]
fn protected_runtime_env_filters_sensitive_and_reserved_inherited_vars() {
    let mut overlay = HashMap::new();
    overlay.insert(
        env::ENV_POLICY_INHERIT_MODE_KEY_FOR_TEST.to_string(),
        env::ENV_POLICY_INHERIT_ALL_FOR_TEST.to_string(),
    );

    let old_path = std::env::var("PATH").ok();
    let old_token = std::env::var("PARENT_TOKEN").ok();
    let old_reserved = std::env::var("HIVEMIND_TASK_ID").ok();
    std::env::set_var("PATH", "/tmp/hardened-path");
    std::env::set_var("PARENT_TOKEN", "super-secret");
    std::env::set_var("HIVEMIND_TASK_ID", "injected");

    let built = build_protected_runtime_environment(&overlay).expect("env build should work");

    match old_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    match old_token {
        Some(value) => std::env::set_var("PARENT_TOKEN", value),
        None => std::env::remove_var("PARENT_TOKEN"),
    }
    match old_reserved {
        Some(value) => std::env::set_var("HIVEMIND_TASK_ID", value),
        None => std::env::remove_var("HIVEMIND_TASK_ID"),
    }

    assert_eq!(built.provenance.inherit_mode, RuntimeEnvInheritMode::All);
    assert!(built
        .provenance
        .dropped_sensitive_inherited_keys
        .iter()
        .any(|key| key == "PARENT_TOKEN"));
    assert!(built
        .provenance
        .dropped_reserved_inherited_keys
        .iter()
        .any(|key| key == "HIVEMIND_TASK_ID"));
    assert!(!built.env.contains_key("PARENT_TOKEN"));
    assert!(!built.env.contains_key("HIVEMIND_TASK_ID"));
    assert_eq!(
        built.env.get("PATH"),
        Some(&"/tmp/hardened-path".to_string())
    );
}

#[test]
fn mock_adapter_lifecycle() {
    let mut adapter = MockAdapter::new();
    assert!(adapter.initialize().is_ok());
    adapter
        .prepare(Uuid::new_v4(), &PathBuf::from("/tmp/test"))
        .unwrap();

    let report = adapter
        .execute(ExecutionInput {
            task_description: "Test task".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
            native_prompt_metadata: None,
        })
        .unwrap();
    assert!(report.is_success());
    adapter.terminate().unwrap();
}

#[test]
fn mock_adapter_custom_response() {
    let custom_report = ExecutionReport::failure(
        1,
        Duration::from_secs(3),
        RuntimeError::new("custom", "Custom error", true),
    );
    let mut adapter = MockAdapter::new().with_response(custom_report);
    adapter
        .prepare(Uuid::new_v4(), &PathBuf::from("/tmp"))
        .unwrap();

    let report = adapter
        .execute(ExecutionInput {
            task_description: "Test".to_string(),
            success_criteria: "Done".to_string(),
            context: None,
            prior_attempts: Vec::new(),
            verifier_feedback: None,
            native_prompt_metadata: None,
        })
        .unwrap();
    assert!(!report.is_success());
}

#[test]
fn runtime_error_types() {
    let timeout = RuntimeError::timeout(Duration::from_secs(60));
    assert_eq!(timeout.code, "timeout");
    assert!(timeout.recoverable);

    let crash = RuntimeError::crash("Segmentation fault");
    assert_eq!(crash.code, "crash");
    assert!(!crash.recoverable);
}
