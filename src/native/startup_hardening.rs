//! Pre-main host/process hardening for native runtime safety.

use serde::Serialize;

const STARTUP_HARDENING_EXIT_CODE: i32 = 70;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupHardeningReport {
    pub cleared_loader_env_vars: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupHardeningFailure {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub exit_code: i32,
}

impl StartupHardeningFailure {
    fn new(code: &str, message: String, stage: &str) -> Self {
        Self {
            code: code.to_string(),
            message,
            stage: stage.to_string(),
            exit_code: STARTUP_HARDENING_EXIT_CODE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StartupHardeningEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub code: String,
    pub message: String,
    pub stage: String,
    pub exit_code: i32,
}

impl StartupHardeningEvent {
    #[must_use]
    pub fn failure(failure: &StartupHardeningFailure) -> Self {
        Self {
            event_type: "startup_hardening_failed".to_string(),
            code: failure.code.clone(),
            message: failure.message.clone(),
            stage: failure.stage.clone(),
            exit_code: failure.exit_code,
        }
    }
}

pub fn emit_startup_hardening_failure_event(failure: &StartupHardeningFailure) {
    let event = StartupHardeningEvent::failure(failure);
    match serde_json::to_string(&event) {
        Ok(encoded) => eprintln!("{encoded}"),
        Err(error) => {
            eprintln!(
                "{{\"type\":\"startup_hardening_failed\",\"code\":\"{}\",\"message\":\"{}\",\"stage\":\"{}\",\"exit_code\":{}}}",
                failure.code,
                format!("{} (event_encode_failed: {error})", failure.message).replace('"', "'"),
                failure.stage,
                failure.exit_code
            );
        }
    }
}

pub fn apply_pre_main_hardening() -> Result<StartupHardeningReport, StartupHardeningFailure> {
    apply_pre_main_hardening_with_ops(&SystemStartupHardeningOps)
}

trait StartupHardeningOps {
    fn disable_core_dumps(&self) -> Result<(), String>;
    fn deny_debugger_attach(&self) -> Result<(), String>;
    fn loader_env_keys(&self) -> Vec<String>;
    fn remove_env_key(&self, key: &str);
}

struct SystemStartupHardeningOps;

impl StartupHardeningOps for SystemStartupHardeningOps {
    fn disable_core_dumps(&self) -> Result<(), String> {
        #[cfg(unix)]
        {
            use nix::sys::resource::{setrlimit, Resource};

            setrlimit(Resource::RLIMIT_CORE, 0, 0)
                .map_err(|error| format!("setrlimit(RLIMIT_CORE=0) failed: {error}"))
        }

        #[cfg(not(unix))]
        {
            Ok(())
        }
    }

    fn deny_debugger_attach(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        {
            use nix::sys::prctl;
            prctl::set_dumpable(false)
                .map_err(|error| format!("prctl(PR_SET_DUMPABLE=0) failed: {error}"))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(())
        }
    }

    fn loader_env_keys(&self) -> Vec<String> {
        std::env::vars()
            .map(|(key, _)| key)
            .filter(|key| is_loader_env_key(key))
            .collect()
    }

    fn remove_env_key(&self, key: &str) {
        std::env::remove_var(key);
    }
}

fn apply_pre_main_hardening_with_ops<O: StartupHardeningOps>(
    ops: &O,
) -> Result<StartupHardeningReport, StartupHardeningFailure> {
    ops.disable_core_dumps().map_err(|message| {
        StartupHardeningFailure::new(
            "startup_hardening_core_dumps_failed",
            message,
            "disable_core_dumps",
        )
    })?;

    ops.deny_debugger_attach().map_err(|message| {
        StartupHardeningFailure::new(
            "startup_hardening_debugger_deny_failed",
            message,
            "deny_debugger_attach",
        )
    })?;

    let mut loader_keys = ops.loader_env_keys();
    loader_keys.sort();
    loader_keys.dedup();
    for key in &loader_keys {
        ops.remove_env_key(key);
    }

    Ok(StartupHardeningReport {
        cleared_loader_env_vars: loader_keys,
    })
}

fn is_loader_env_key(key: &str) -> bool {
    key.starts_with("LD_") || key.starts_with("DYLD_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct MockStartupHardeningOps {
        disable_result: Result<(), String>,
        deny_result: Result<(), String>,
        loader_keys: Vec<String>,
        removed_keys: RefCell<Vec<String>>,
        disable_calls: Cell<usize>,
        deny_calls: Cell<usize>,
    }

    impl MockStartupHardeningOps {
        fn new(disable_result: Result<(), String>, deny_result: Result<(), String>) -> Self {
            Self {
                disable_result,
                deny_result,
                loader_keys: Vec::new(),
                removed_keys: RefCell::new(Vec::new()),
                disable_calls: Cell::new(0),
                deny_calls: Cell::new(0),
            }
        }
    }

    impl StartupHardeningOps for MockStartupHardeningOps {
        fn disable_core_dumps(&self) -> Result<(), String> {
            self.disable_calls.set(self.disable_calls.get() + 1);
            self.disable_result.clone()
        }

        fn deny_debugger_attach(&self) -> Result<(), String> {
            self.deny_calls.set(self.deny_calls.get() + 1);
            self.deny_result.clone()
        }

        fn loader_env_keys(&self) -> Vec<String> {
            self.loader_keys.clone()
        }

        fn remove_env_key(&self, key: &str) {
            self.removed_keys.borrow_mut().push(key.to_string());
        }
    }

    #[test]
    fn fail_fast_when_core_dump_hardening_fails() {
        let ops = MockStartupHardeningOps::new(Err("no permissions".to_string()), Ok(()));
        let failure = apply_pre_main_hardening_with_ops(&ops).expect_err("must fail");
        assert_eq!(failure.code, "startup_hardening_core_dumps_failed");
        assert_eq!(ops.disable_calls.get(), 1);
        assert_eq!(ops.deny_calls.get(), 0);
        assert!(ops.removed_keys.borrow().is_empty());
    }

    #[test]
    fn fail_fast_when_debugger_deny_fails() {
        let ops = MockStartupHardeningOps::new(Ok(()), Err("blocked".to_string()));
        let failure = apply_pre_main_hardening_with_ops(&ops).expect_err("must fail");
        assert_eq!(failure.code, "startup_hardening_debugger_deny_failed");
        assert_eq!(ops.disable_calls.get(), 1);
        assert_eq!(ops.deny_calls.get(), 1);
        assert!(ops.removed_keys.borrow().is_empty());
    }

    #[test]
    fn success_clears_loader_env_keys() {
        let mut ops = MockStartupHardeningOps::new(Ok(()), Ok(()));
        ops.loader_keys = vec![
            "DYLD_INSERT_LIBRARIES".to_string(),
            "LD_PRELOAD".to_string(),
        ];

        let report = apply_pre_main_hardening_with_ops(&ops).expect("must succeed");
        assert_eq!(
            report.cleared_loader_env_vars,
            vec![
                "DYLD_INSERT_LIBRARIES".to_string(),
                "LD_PRELOAD".to_string()
            ]
        );
        assert_eq!(
            *ops.removed_keys.borrow(),
            vec![
                "DYLD_INSERT_LIBRARIES".to_string(),
                "LD_PRELOAD".to_string()
            ]
        );
    }

    #[test]
    fn startup_failure_event_is_structured_json() {
        let failure = StartupHardeningFailure::new(
            "startup_hardening_core_dumps_failed",
            "setrlimit failed".to_string(),
            "disable_core_dumps",
        );
        let event = StartupHardeningEvent::failure(&failure);
        let encoded = serde_json::to_string(&event).expect("event json");
        assert!(encoded.contains("\"type\":\"startup_hardening_failed\""));
        assert!(encoded.contains("\"code\":\"startup_hardening_core_dumps_failed\""));
    }
}
