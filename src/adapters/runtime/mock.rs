use super::{AdapterConfig, ExecutionInput, ExecutionReport, RuntimeAdapter, RuntimeError};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug)]
pub struct MockAdapter {
    config: AdapterConfig,
    prepared: bool,
    response: Option<ExecutionReport>,
}

impl MockAdapter {
    pub fn new() -> Self {
        Self {
            config: AdapterConfig::new("mock", PathBuf::from("/bin/echo")),
            prepared: false,
            response: None,
        }
    }

    #[must_use]
    pub fn with_response(mut self, report: ExecutionReport) -> Self {
        self.response = Some(report);
        self
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for MockAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn initialize(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn prepare(&mut self, _task_id: Uuid, _worktree: &Path) -> Result<(), RuntimeError> {
        self.prepared = true;
        Ok(())
    }

    fn execute(&mut self, _input: ExecutionInput) -> Result<ExecutionReport, RuntimeError> {
        if !self.prepared {
            return Err(RuntimeError::new(
                "not_prepared",
                "Adapter not prepared",
                false,
            ));
        }

        Ok(self.response.clone().unwrap_or_else(|| {
            ExecutionReport::success(
                Duration::from_secs(1),
                "mock output".to_string(),
                String::new(),
            )
        }))
    }

    fn terminate(&mut self) -> Result<(), RuntimeError> {
        self.prepared = false;
        Ok(())
    }

    fn config(&self) -> &AdapterConfig {
        &self.config
    }
}
