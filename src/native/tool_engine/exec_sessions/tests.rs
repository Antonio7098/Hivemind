use super::*;

#[test]
fn parse_session_cap_falls_back_for_invalid_values() {
    let mut env = HashMap::new();
    env.insert(EXEC_SESSION_CAP_ENV_KEY.to_string(), "0".to_string());
    assert_eq!(support::parse_session_cap(&env), DEFAULT_EXEC_SESSION_CAP);
}

#[test]
fn clamp_exec_wait_ms_respects_timeout_envelope() {
    let started = Instant::now() - Duration::from_millis(900);
    let wait = support::clamp_exec_wait_ms(Some(500), 80, 1_000, started);
    assert!(wait <= 50);
    assert!(wait >= 1);
}
