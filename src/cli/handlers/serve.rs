//! Serve command handlers.

use crate::cli::commands::ServeArgs;
use crate::cli::output::{output_error, OutputFormat};
use crate::core::error::ExitCode;

pub fn handle_serve(args: ServeArgs, format: OutputFormat) -> ExitCode {
    if format != OutputFormat::Table {
        eprintln!("Warning: 'serve' always logs to stderr; output format is ignored.");
    }

    let config = crate::server::ServeConfig {
        host: args.host,
        port: args.port,
        events_limit: args.events_limit,
    };

    match crate::server::serve(&config) {
        Ok(()) => ExitCode::Success,
        Err(e) => output_error(&e, format),
    }
}
