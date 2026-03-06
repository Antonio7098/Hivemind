//! CLI output formatting helpers.
//!
//! This module contains utilities for parsing and rendering
//! different output formats, extracted from main.rs.

use crate::cli::output::{output, OutputFormat};
use std::ffi::OsString;

/// Parse output format from command line arguments.
pub fn parse_format_from_args(args: &[OsString]) -> OutputFormat {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        let s = arg.to_string_lossy();

        if s == "-f" || s == "--format" {
            if let Some(value) = iter.next() {
                return parse_format_value(&value.to_string_lossy());
            }
        }

        if let Some(value) = s.strip_prefix("--format=") {
            return parse_format_value(value);
        }
    }

    OutputFormat::Table
}

/// Parse format value string.
pub fn parse_format_value(value: &str) -> OutputFormat {
    let v = value.to_lowercase();
    if v == "json" {
        OutputFormat::Json
    } else if v == "yaml" || v == "yml" {
        OutputFormat::Yaml
    } else {
        OutputFormat::Table
    }
}

/// Print structured data in the specified format.
pub fn print_structured<T: serde::Serialize>(value: &T, format: OutputFormat, context: &str) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(value) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(value, format) {
                eprintln!("Failed to render {context}: {err}");
            }
        }
    }
}

/// Output help text in the specified format.
pub fn output_help(help: &str, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            print!("{help}");
        }
        OutputFormat::Json => {
            let response = crate::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = crate::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}

/// Output version information in the specified format.
pub fn output_version(format: OutputFormat) {
    let version = env!("CARGO_PKG_VERSION");
    match format {
        OutputFormat::Table => {
            println!("hivemind {version}");
        }
        OutputFormat::Json => {
            let response = crate::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = crate::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}
