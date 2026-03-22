use super::*;

#[test]
fn classify_runtime_error_detects_rate_limits() {
    let classified = Registry::classify_runtime_error(
        "native_transport_failed",
        "Too many requests",
        false,
        "",
        "http 429",
    );
    assert_eq!(classified.category, "rate_limit");
    assert!(classified.recoverable);
    assert!(classified.retryable);
    assert!(classified.rate_limited);
}

#[test]
fn detect_runtime_output_failure_reports_auth_errors() {
    let failure = Registry::detect_runtime_output_failure("", "Authentication failed")
        .expect("should classify auth failure");
    assert_eq!(failure.0, "runtime_auth_failed");
    assert!(!failure.2);
}

#[test]
fn classify_runtime_error_treats_no_progress_timeout_as_retryable_timeout() {
    let classified = Registry::classify_runtime_error(
        "no_observable_progress_timeout",
        "Runtime produced no observable progress",
        true,
        "",
        "",
    );
    assert_eq!(classified.category, "timeout");
    assert!(classified.recoverable);
    assert!(classified.retryable);
    assert!(!classified.rate_limited);
}
