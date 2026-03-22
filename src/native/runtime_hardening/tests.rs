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

#[test]
#[cfg(unix)]
fn cleanup_native_blob_storage_prunes_expired_blobs_and_empty_dirs() {
    let dir = tempdir().expect("temp dir");
    let blob_root = dir.path().join("blobs").join("sha256");
    let kept_dir = blob_root.join("aa");
    let pruned_dir = blob_root.join("bb");
    fs::create_dir_all(&kept_dir).expect("create kept dir");
    fs::create_dir_all(&pruned_dir).expect("create pruned dir");
    let kept = kept_dir.join("keep.blob");
    let expired = pruned_dir.join("drop.blob");
    fs::write(&kept, b"keep").expect("write kept blob");
    fs::write(&expired, b"drop").expect("write expired blob");

    let old_stamp = Command::new("touch")
        .args([
            "-d",
            "2000-01-01T00:00:00Z",
            expired.to_str().expect("expired path"),
        ])
        .status()
        .expect("touch old blob");
    assert!(old_stamp.success());

    let removed = cleanup_native_blob_storage(&blob_root, 30).expect("cleanup blobs");
    assert_eq!(removed, 1);
    assert!(kept.exists(), "recent blob should remain");
    assert!(!expired.exists(), "expired blob should be removed");
    assert!(
        !pruned_dir.exists(),
        "empty blob prefix dir should be removed"
    );
}
