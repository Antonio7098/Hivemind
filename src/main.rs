//! Hivemind CLI entrypoint.

use clap::error::ErrorKind;
use clap::CommandFactory;
use clap::Parser;
use hivemind::cli::commands::{Cli, Commands};
use hivemind::cli::output::OutputFormat;
use hivemind::core::error::ExitCode;
use hivemind::native::startup_hardening;
use std::ffi::OsString;
use std::process;

fn parse_format_from_args(args: &[OsString]) -> OutputFormat {
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

fn parse_format_value(value: &str) -> OutputFormat {
    let v = value.to_lowercase();
    if v == "json" {
        OutputFormat::Json
    } else if v == "yaml" || v == "yml" {
        OutputFormat::Yaml
    } else {
        OutputFormat::Table
    }
}

fn output_help(help: &str, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            print!("{help}");
        }
        OutputFormat::Json => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "help": help
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}

fn output_version(format: OutputFormat) {
    let version = env!("CARGO_PKG_VERSION");
    match format {
        OutputFormat::Table => {
            println!("hivemind {version}");
        }
        OutputFormat::Json => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(json) = serde_json::to_string_pretty(&response) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let response = hivemind::cli::output::CliResponse::success(serde_json::json!({
                "name": "hivemind",
                "version": version
            }));
            if let Ok(yaml) = serde_yaml::to_string(&response) {
                print!("{yaml}");
            }
        }
    }
}

fn handle_clap_error(err: &clap::Error, format: OutputFormat) -> ExitCode {
    match err.kind() {
        ErrorKind::DisplayHelp => {
            let rendered = err.render().to_string();
            output_help(&rendered, format);
            ExitCode::Success
        }
        ErrorKind::DisplayVersion => {
            output_version(format);
            ExitCode::Success
        }
        _ => {
            eprintln!("{}", err.render());
            ExitCode::Error
        }
    }
}

fn main() {
    if let Err(failure) = startup_hardening::apply_pre_main_hardening() {
        startup_hardening::emit_startup_hardening_failure_event(&failure);
        process::exit(i32::from(ExitCode::StartupHardeningFailed));
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let is_broken_pipe = info
            .payload()
            .downcast_ref::<&str>()
            .is_some_and(|s| s.contains("Broken pipe"))
            || info
                .payload()
                .downcast_ref::<String>()
                .is_some_and(|s| s.contains("Broken pipe"));

        if is_broken_pipe {
            return;
        }

        default_hook(info);
    }));

    let args: Vec<OsString> = std::env::args_os().collect();
    let format = parse_format_from_args(&args);

    let result = std::panic::catch_unwind(|| Cli::try_parse_from(&args).map(run));

    match result {
        Ok(Ok(exit_code)) => process::exit(i32::from(exit_code)),
        Ok(Err(e)) => process::exit(i32::from(handle_clap_error(&e, format))),
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("panic");

            if msg.contains("Broken pipe") {
                process::exit(0);
            }

            std::panic::resume_unwind(payload);
        }
    }
}

fn run(cli: Cli) -> ExitCode {
    let format = cli.format;

    match cli.command {
        Some(Commands::Version) => {
            output_version(format);
            ExitCode::Success
        }
        Some(Commands::Serve(args)) => hivemind::cli::handlers::handle_serve(args, format),
        Some(Commands::Project(cmd)) => hivemind::cli::handlers::handle_project(cmd, format),
        Some(Commands::Global(cmd)) => hivemind::cli::handlers::handle_global(cmd, format),
        Some(Commands::Constitution(cmd)) => {
            hivemind::cli::handlers::handle_constitution(cmd, format)
        }
        Some(Commands::Task(cmd)) => hivemind::cli::handlers::handle_task(cmd, format),
        Some(Commands::Graph(cmd)) => hivemind::cli::handlers::handle_graph(cmd, format),
        Some(Commands::Flow(cmd)) => hivemind::cli::handlers::handle_flow(cmd, format),
        Some(Commands::Events(cmd)) => hivemind::cli::handlers::handle_events(cmd, format),
        Some(Commands::Runtime(cmd)) => hivemind::cli::handlers::handle_runtime(cmd, format),
        Some(Commands::Verify(cmd)) => hivemind::cli::handlers::handle_verify(cmd, format),
        Some(Commands::Merge(cmd)) => hivemind::cli::handlers::handle_merge(cmd, format),
        Some(Commands::Attempt(cmd)) => hivemind::cli::handlers::handle_attempt(cmd, format),
        Some(Commands::Checkpoint(cmd)) => hivemind::cli::handlers::handle_checkpoint(cmd, format),
        Some(Commands::Worktree(cmd)) => hivemind::cli::handlers::handle_worktree(cmd, format),
        None => {
            if let Err(err) = Cli::command().print_help() {
                eprintln!("Failed to print help: {err}");
                return ExitCode::Error;
            }
            println!();
            ExitCode::Success
        }
    }
}
