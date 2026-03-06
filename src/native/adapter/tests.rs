use super::*;
use tempfile::tempdir;

fn basic_input() -> ExecutionInput {
    ExecutionInput {
        task_description: "list files".to_string(),
        success_criteria: "show files".to_string(),
        context: None,
        prior_attempts: Vec::new(),
        verifier_feedback: None,
    }
}

#[test]
fn execute_requires_prepare() {
    let mut adapter = NativeRuntimeAdapter::new(NativeAdapterConfig::default());
    let error = adapter
        .execute(basic_input())
        .expect_err("should require prepare");
    assert_eq!(error.code, "not_prepared");
}

#[test]
fn invalid_scope_json_is_reported() {
    let mut config = NativeAdapterConfig::default();
    config
        .base
        .env
        .insert("HIVEMIND_TASK_SCOPE_JSON".to_string(), "{".to_string());
    let mut adapter = NativeRuntimeAdapter::new(config);
    let dir = tempdir().expect("tempdir");
    adapter.prepare(Uuid::new_v4(), dir.path()).unwrap();
    let error = adapter
        .execute(basic_input())
        .expect_err("invalid scope json should fail");
    assert_eq!(error.code, "native_scope_decode_failed");
}
