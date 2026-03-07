//! CLI error handling utilities.
//!
//! This module contains error handling functions extracted from main.rs.

use crate::core::error::ExitCode;
use clap::error::ErrorKind;

/// Handle clap errors with appropriate output formatting.
pub fn handle_clap_error(err: &clap::Error, format: crate::cli::output::OutputFormat) -> ExitCode {
    match err.kind() {
        ErrorKind::DisplayHelp => {
            let rendered = err.render().to_string();
            crate::cli::output_helpers::output_help(&rendered, format);
            ExitCode::Success
        }
        ErrorKind::DisplayVersion => {
            crate::cli::output_helpers::output_version(format);
            ExitCode::Success
        }
        _ => {
            eprintln!("{}", err.render());
            ExitCode::Error
        }
    }
}
