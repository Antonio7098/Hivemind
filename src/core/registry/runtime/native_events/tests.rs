use super::*;
use crate::core::registry::RegistryConfig;
use tempfile::tempdir;

#[test]
fn native_blob_sha256_is_stable() {
    let digest = Registry::native_blob_sha256(b"hello world");
    assert_eq!(digest.len(), 64);
    assert_eq!(digest, Registry::native_blob_sha256(b"hello world"));
}

#[test]
fn native_capture_mode_maps_to_event_mode() {
    assert_eq!(
        Registry::native_capture_mode_for_event(NativePayloadCaptureMode::MetadataOnly),
        NativeEventPayloadCaptureMode::MetadataOnly
    );
    assert_eq!(
        Registry::native_capture_mode_for_event(NativePayloadCaptureMode::FullPayload),
        NativeEventPayloadCaptureMode::FullPayload
    );
}

#[test]
fn persist_native_blob_omits_oversized_inline_payloads() {
    let tmp = tempdir().expect("tempdir");
    let registry = Registry::open_with_config(RegistryConfig::with_dir(tmp.path().join("data")))
        .expect("registry");

    let inline_limit = Registry::native_blob_inline_payload_limit_bytes();
    let small_payload = "a".repeat(inline_limit);
    let small = registry
        .persist_native_blob(
            "text/plain",
            &small_payload,
            NativePayloadCaptureMode::FullPayload,
            "test",
        )
        .expect("small payload blob");
    assert_eq!(small.payload.as_deref(), Some(small_payload.as_str()));

    let large_payload = "b".repeat(inline_limit.saturating_add(1));
    let large = registry
        .persist_native_blob(
            "text/plain",
            &large_payload,
            NativePayloadCaptureMode::FullPayload,
            "test",
        )
        .expect("large payload blob");
    assert!(
        large.payload.is_none(),
        "oversized payload should not be inlined"
    );
    assert_eq!(
        std::fs::read_to_string(&large.blob_path).expect("blob payload should persist"),
        large_payload
    );
}
