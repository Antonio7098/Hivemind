use super::*;

impl NativeRuntimeStateStore {
    pub(super) fn exec(&self, sql: &str) -> Result<()> {
        let output = Command::new("sqlite3")
            .arg("-batch")
            .arg("-cmd")
            .arg(format!(".timeout {}", self.busy_timeout_ms))
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_exec_failed",
                    format!(
                        "Failed to invoke sqlite3 for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            "sqlite3 command failed with empty stderr".to_string()
        } else {
            stderr
        };
        Err(RuntimeHardeningError::new(
            "native_runtime_state_sql_exec_failed",
            format!("sqlite3 failed for '{}': {message}", self.db_path.display()),
        ))
    }

    pub(super) fn scalar_i64(&self, sql: &str) -> Result<i64> {
        let output = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg("-cmd")
            .arg(format!(".timeout {}", self.busy_timeout_ms))
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_query_failed",
                    format!(
                        "Failed to invoke sqlite3 query for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RuntimeHardeningError::new(
                "native_runtime_state_sql_query_failed",
                format!(
                    "sqlite3 query failed for '{}': {}",
                    self.db_path.display(),
                    if stderr.is_empty() {
                        "empty stderr".to_string()
                    } else {
                        stderr
                    }
                ),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let value = raw
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string();
        value.parse::<i64>().map_err(|error| {
            RuntimeHardeningError::new(
                "native_runtime_state_sql_parse_failed",
                format!("Failed to parse sqlite integer output '{raw}': {error}"),
            )
        })
    }

    pub(super) fn scalar_string(&self, sql: &str) -> Result<Option<String>> {
        let output = Command::new("sqlite3")
            .arg("-noheader")
            .arg("-batch")
            .arg("-cmd")
            .arg(format!(".timeout {}", self.busy_timeout_ms))
            .arg(self.db_path.as_os_str())
            .arg(sql)
            .output()
            .map_err(|error| {
                RuntimeHardeningError::new(
                    "native_runtime_state_sql_query_failed",
                    format!(
                        "Failed to invoke sqlite3 query for '{}': {error}",
                        self.db_path.display()
                    ),
                )
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(RuntimeHardeningError::new(
                "native_runtime_state_sql_query_failed",
                format!(
                    "sqlite3 query failed for '{}': {}",
                    self.db_path.display(),
                    if stderr.is_empty() {
                        "empty stderr".to_string()
                    } else {
                        stderr
                    }
                ),
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let value = raw
            .lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default()
            .to_string();
        if value.is_empty() {
            Ok(None)
        } else {
            Ok(Some(value))
        }
    }
}
