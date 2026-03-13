use super::*;

pub(super) fn status_with_retry(
    binary: &std::path::Path,
    arg: &str,
    timeout: Duration,
) -> std::io::Result<std::process::ExitStatus> {
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..50 {
        let mut child = match Command::new(binary)
            .arg(arg)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(e) if e.raw_os_error() == Some(26) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(e) => return Err(e),
        };

        let start = std::time::Instant::now();
        loop {
            match child.try_wait()? {
                Some(status) => return Ok(status),
                None if start.elapsed() >= timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("Timed out waiting for {} {}", binary.display(), arg),
                    ));
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }

    Err(last_err.unwrap_or_else(|| std::io::Error::other("Executable remained busy")))
}
/// `OpenCode` adapter configuration.
#[derive(Debug, Clone)]
pub struct OpenCodeConfig {
    /// Base adapter config.
    pub base: AdapterConfig,
    /// Model to use (if configurable).
    pub model: Option<String>,
    /// Whether to enable verbose output.
    pub verbose: bool,
}
impl OpenCodeConfig {
    /// Creates a new `OpenCode` config with default settings.
    pub fn new(binary_path: PathBuf) -> Self {
        let model = env::var("HIVEMIND_OPENCODE_MODEL").ok();
        Self {
            base: AdapterConfig::new("opencode", binary_path)
                .with_timeout(Duration::from_secs(600)), // 10 minutes
            model,
            verbose: false,
        }
    }

    /// Sets the model.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Enables verbose mode.
    #[must_use]
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Sets the timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.base.timeout = timeout;
        self
    }
}
impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self::new(PathBuf::from("opencode"))
    }
}
