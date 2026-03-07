use super::*;

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
