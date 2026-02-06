//! CLI output formatting (JSON, table).
//!
//! All CLI output supports structured formats for machine consumption.

use crate::core::error::{ExitCode, HivemindError};
use comfy_table::{Cell, Table};
use serde::Serialize;

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format.
    #[default]
    Table,
    /// Machine-readable JSON format.
    Json,
    /// YAML output format.
    Yaml,
}

/// Structured CLI response.
#[derive(Debug, Serialize)]
pub struct CliResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorOutput>,
}

/// Structured error output.
#[derive(Debug, Serialize)]
pub struct ErrorOutput {
    pub category: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl From<&HivemindError> for ErrorOutput {
    fn from(err: &HivemindError) -> Self {
        Self {
            category: err.category.to_string(),
            code: err.code.clone(),
            message: err.message.clone(),
            hint: err.recovery_hint.clone(),
        }
    }
}

impl<T: Serialize> CliResponse<T> {
    /// Creates a successful response with data.
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Creates an error response.
    pub fn error(err: &HivemindError) -> CliResponse<()> {
        CliResponse {
            success: false,
            data: None,
            error: Some(ErrorOutput::from(err)),
        }
    }
}

/// Outputs data in the specified format.
pub fn output<T: Serialize>(data: T, format: OutputFormat) -> std::io::Result<()> {
    match format {
        OutputFormat::Json => {
            let response = CliResponse::success(data);
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        OutputFormat::Table => {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        OutputFormat::Yaml => {
            let response = CliResponse::success(data);
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
    Ok(())
}

/// Outputs an error in the specified format.
pub fn output_error(err: &HivemindError, format: OutputFormat) -> ExitCode {
    match format {
        OutputFormat::Json => {
            let response = CliResponse::<()>::error(err);
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                eprintln!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = CliResponse::<()>::error(err);
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                eprint!("{yaml}");
            }
        }
        OutputFormat::Table => {
            eprintln!("Error: {err}");
            if let Some(hint) = &err.recovery_hint {
                eprintln!("Hint: {hint}");
            }
        }
    }
    error_to_exit_code(err)
}

/// Maps error codes to exit codes per CLI operational semantics.
fn error_to_exit_code(err: &HivemindError) -> ExitCode {
    match err.code.as_str() {
        c if c.contains("not_found") => ExitCode::NotFound,
        c if c.contains("conflict")
            || c.contains("immutable")
            || c.contains("in_use")
            || c.contains("already_terminal")
            || c.contains("already_running")
            || c.contains("not_running")
            || c.contains("not_paused") =>
        {
            ExitCode::Conflict
        }
        "override_not_permitted" => ExitCode::Conflict,
        _ => ExitCode::Error,
    }
}

/// Helper to create a table with headers.
#[must_use]
pub fn create_table(headers: &[&str]) -> Table {
    let mut table = Table::new();
    table.set_header(headers.iter().map(|h| Cell::new(*h)));
    table
}

/// Trait for types that can be displayed as a table row.
pub trait TableRow {
    fn to_row(&self) -> Vec<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn cli_response_success_serialization() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let response = CliResponse::success(data);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"name\":\"test\""));
    }

    #[test]
    fn cli_response_error_serialization() {
        let err =
            HivemindError::user("invalid", "Invalid input", "cli:test").with_hint("Try again");
        let response = CliResponse::<()>::error(&err);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains("\"success\":false"));
        assert!(json.contains("\"code\":\"invalid\""));
    }
}
