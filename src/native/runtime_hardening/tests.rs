use super::*;
use tempfile::tempdir;

#[test]
fn readiness_transitions_are_recorded_for_all_components() {
    let dir = tempdir().expect("temp dir");
    let mut env = HashMap::new();
    env.insert(
        STATE_DB_PATH_ENV.to_string(),
        dir.path()
            .join("state.sqlite")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        SECRETS_STORE_PATH_ENV.to_string(),
        dir.path()
            .join("secrets.enc.json")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        SECRETS_KEYRING_PATH_ENV.to_string(),
        dir.path()
            .join("secrets.keyring")
            .to_string_lossy()
            .to_string(),
    );

    let support = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap should pass");
    let transitions = support.readiness_transitions();
    assert!(transitions
        .iter()
        .any(|transition| transition.component == "runtime_state_db"
            && transition.to_state == "ready"));
    assert!(transitions
        .iter()
        .any(|transition| transition.component == "secrets_manager"
            && transition.to_state == "ready"));
    assert!(transitions
        .iter()
        .any(|transition| transition.component == "runtime_log_ingestor"
            && transition.to_state == "ready"));
    support.shutdown().expect("shutdown");
}
