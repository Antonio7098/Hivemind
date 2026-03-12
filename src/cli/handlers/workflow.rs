//! Workflow command handlers.

use crate::cli::commands::{WorkflowCommands, WorkflowStepKindArg, WorkflowStepStateArg};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;
use crate::core::workflow::{WorkflowDefinition, WorkflowRun, WorkflowStepKind, WorkflowStepState};

fn get_registry(format: OutputFormat) -> Option<Registry> {
    match Registry::open() {
        Ok(r) => Some(r),
        Err(e) => {
            output_error(&e, format);
            None
        }
    }
}

fn print_workflows(workflows: &[WorkflowDefinition], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if workflows.is_empty() {
                println!("No workflows found.");
                return;
            }
            println!("{:<36}  {:<36}  {:<24}  STEPS", "ID", "PROJECT", "NAME");
            println!("{}", "-".repeat(114));
            for workflow in workflows {
                println!(
                    "{:<36}  {:<36}  {:<24}  {}",
                    workflow.id,
                    workflow.project_id,
                    workflow.name,
                    workflow.steps.len()
                );
            }
        }
        _ => {
            if let Err(err) = output(workflows, format) {
                eprintln!("Failed to render workflows: {err}");
            }
        }
    }
}

fn print_workflow_runs(runs: &[WorkflowRun], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if runs.is_empty() {
                println!("No workflow runs found.");
                return;
            }
            println!(
                "{:<36}  {:<36}  {:<36}  {:<10}",
                "RUN", "WORKFLOW", "ROOT", "STATE"
            );
            println!("{}", "-".repeat(130));
            for run in runs {
                println!(
                    "{:<36}  {:<36}  {:<36}  {:<10}",
                    run.id,
                    run.workflow_id,
                    run.root_workflow_run_id,
                    format!("{:?}", run.state).to_lowercase()
                );
            }
        }
        _ => {
            if let Err(err) = output(runs, format) {
                eprintln!("Failed to render workflow runs: {err}");
            }
        }
    }
}

fn output_workflow_id(workflow_id: impl serde::Serialize, format: OutputFormat) -> ExitCode {
    if let Err(err) = output(serde_json::json!({ "workflow_id": workflow_id }), format) {
        eprintln!("Failed to render workflow id: {err}");
    }
    ExitCode::Success
}

fn output_workflow_run_id(run_id: impl serde::Serialize, format: OutputFormat) -> ExitCode {
    if let Err(err) = output(serde_json::json!({ "workflow_run_id": run_id }), format) {
        eprintln!("Failed to render workflow run id: {err}");
    }
    ExitCode::Success
}

fn workflow_step_kind(kind: WorkflowStepKindArg) -> WorkflowStepKind {
    match kind {
        WorkflowStepKindArg::Task => WorkflowStepKind::Task,
        WorkflowStepKindArg::Workflow => WorkflowStepKind::Workflow,
        WorkflowStepKindArg::Conditional => WorkflowStepKind::Conditional,
        WorkflowStepKindArg::Wait => WorkflowStepKind::Wait,
        WorkflowStepKindArg::Join => WorkflowStepKind::Join,
    }
}

fn workflow_step_state(state: WorkflowStepStateArg) -> WorkflowStepState {
    match state {
        WorkflowStepStateArg::Pending => WorkflowStepState::Pending,
        WorkflowStepStateArg::Ready => WorkflowStepState::Ready,
        WorkflowStepStateArg::Running => WorkflowStepState::Running,
        WorkflowStepStateArg::Verifying => WorkflowStepState::Verifying,
        WorkflowStepStateArg::Retry => WorkflowStepState::Retry,
        WorkflowStepStateArg::Waiting => WorkflowStepState::Waiting,
        WorkflowStepStateArg::Succeeded => WorkflowStepState::Succeeded,
        WorkflowStepStateArg::Failed => WorkflowStepState::Failed,
        WorkflowStepStateArg::Skipped => WorkflowStepState::Skipped,
        WorkflowStepStateArg::Aborted => WorkflowStepState::Aborted,
    }
}

fn render_workflow(workflow: WorkflowDefinition, format: OutputFormat) -> ExitCode {
    if format == OutputFormat::Table {
        println!("ID:          {}", workflow.id);
        println!("Project:     {}", workflow.project_id);
        println!("Name:        {}", workflow.name);
        println!("Description: {}", workflow.description.unwrap_or_default());
        println!("Steps:       {}", workflow.steps.len());
        for step in workflow.steps.values() {
            let dependencies = step
                .depends_on
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "  - {} [{}] deps={} id={}",
                step.name,
                format!("{:?}", step.kind).to_lowercase(),
                if dependencies.is_empty() {
                    "-"
                } else {
                    &dependencies
                },
                step.id
            );
        }
    } else if let Err(err) = output(&workflow, format) {
        eprintln!("Failed to render workflow: {err}");
    }
    ExitCode::Success
}

fn render_workflow_run(registry: &Registry, run: WorkflowRun, format: OutputFormat) -> ExitCode {
    if format == OutputFormat::Table {
        let workflow = registry.get_workflow(&run.workflow_id.to_string()).ok();
        println!("Run:      {}", run.id);
        println!("Workflow: {}", run.workflow_id);
        println!("Project:  {}", run.project_id);
        println!("Root:     {}", run.root_workflow_run_id);
        println!("State:    {:?}", run.state);
        println!("Steps:    {}", run.step_runs.len());
        for (step_id, step_run) in &run.step_runs {
            let step_name = workflow
                .as_ref()
                .and_then(|definition| definition.steps.get(step_id))
                .map_or("<unknown>", |step| step.name.as_str());
            println!(
                "  - {} [{}] step={} step_run={}",
                step_name,
                format!("{:?}", step_run.state).to_lowercase(),
                step_run.step_id,
                step_run.id
            );
        }
    } else if let Err(err) = output(&run, format) {
        eprintln!("Failed to render workflow run: {err}");
    }
    ExitCode::Success
}

#[allow(clippy::too_many_lines)]
pub fn handle_workflow(cmd: WorkflowCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        WorkflowCommands::Create(args) => {
            match registry.create_workflow(&args.project, &args.name, args.description.as_deref()) {
                Ok(workflow) => output_workflow_id(workflow.id, format),
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::Update(args) => match registry.update_workflow(
            &args.workflow_id,
            args.name.as_deref(),
            args.description.as_deref(),
            args.clear_description,
        ) {
            Ok(workflow) => render_workflow(workflow, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::StepAdd(args) => match registry.workflow_add_step(
            &args.workflow_id,
            &args.name,
            workflow_step_kind(args.kind),
            args.description.as_deref(),
            &args.depends_on,
        ) {
            Ok(workflow) => render_workflow(workflow, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::List(args) => match registry.list_workflows(args.project.as_deref()) {
            Ok(workflows) => {
                print_workflows(&workflows, format);
                ExitCode::Success
            }
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::Inspect(args) => match registry.get_workflow(&args.workflow_id) {
            Ok(workflow) => render_workflow(workflow, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::RunCreate(args) => {
            match registry.create_workflow_run(&args.workflow_id) {
                Ok(run) => output_workflow_run_id(run.id, format),
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::RunList(args) => {
            match registry.list_workflow_runs(args.project.as_deref(), args.workflow.as_deref()) {
                Ok(runs) => {
                    print_workflow_runs(&runs, format);
                    ExitCode::Success
                }
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::Status(args) => match registry.get_workflow_run(&args.workflow_run_id) {
            Ok(run) => render_workflow_run(&registry, run, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::Start(args) => match registry.start_workflow_run(&args.workflow_run_id) {
            Ok(run) => output_workflow_run_id(run.id, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::Tick(args) => {
            match registry.tick_workflow_run(
                &args.workflow_run_id,
                args.interactive,
                args.max_parallel,
            ) {
                Ok(run) => render_workflow_run(&registry, run, format),
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::Complete(args) => {
            match registry.complete_workflow_run(&args.workflow_run_id) {
                Ok(run) => output_workflow_run_id(run.id, format),
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::Pause(args) => match registry.pause_workflow_run(&args.workflow_run_id) {
            Ok(run) => output_workflow_run_id(run.id, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::Resume(args) => match registry.resume_workflow_run(&args.workflow_run_id)
        {
            Ok(run) => output_workflow_run_id(run.id, format),
            Err(err) => output_error(&err, format),
        },
        WorkflowCommands::Abort(args) => {
            match registry.abort_workflow_run(
                &args.workflow_run_id,
                args.reason.as_deref(),
                args.force,
            ) {
                Ok(run) => output_workflow_run_id(run.id, format),
                Err(err) => output_error(&err, format),
            }
        }
        WorkflowCommands::StepSetState(args) => match registry.workflow_step_set_state(
            &args.workflow_run_id,
            &args.step_id,
            workflow_step_state(args.state),
            args.reason.as_deref(),
        ) {
            Ok(run) => render_workflow_run(&registry, run, format),
            Err(err) => output_error(&err, format),
        },
    }
}
