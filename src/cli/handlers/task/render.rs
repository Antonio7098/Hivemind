use super::*;

pub(super) fn print_task(task: &Task, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("ID:          {}", task.id);
            println!("Project:     {}", task.project_id);
            println!("Title:       {}", task.title);
            if let Some(desc) = &task.description {
                println!("Description: {desc}");
            }
            println!("State:       {:?}", task.state);
            println!("RunMode:     {:?}", task.run_mode);
            println!("Created:     {}", task.created_at);
        }
        _ => {
            if let Err(err) = output(task, format) {
                eprintln!("Failed to render task: {err}");
            }
        }
    }
}
pub(super) fn print_tasks(tasks: &[Task], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if tasks.is_empty() {
                println!("No tasks found.");
                return;
            }
            println!("{:<36}  {:<8}  {:<6}  TITLE", "ID", "STATE", "MODE");
            println!("{}", "-".repeat(80));
            for t in tasks {
                let state = match t.state {
                    TaskState::Open => "open",
                    TaskState::Closed => "closed",
                };
                println!(
                    "{:<36}  {:<8}  {:<6}  {}",
                    t.id,
                    state,
                    format!("{:?}", t.run_mode).to_lowercase(),
                    t.title
                );
            }
        }
        _ => {
            if let Err(err) = output(tasks, format) {
                eprintln!("Failed to render tasks: {err}");
            }
        }
    }
}
pub(super) fn print_task_id(task_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"task_id": task_id}));
        }
        OutputFormat::Table => {
            println!("Task ID: {task_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"task_id": task_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}
pub(super) fn parse_task_state(s: &str) -> Option<TaskState> {
    match s.to_lowercase().as_str() {
        "open" => Some(TaskState::Open),
        "closed" => Some(TaskState::Closed),
        _ => None,
    }
}
pub(super) fn parse_scope_arg(
    scope: Option<&str>,
    format: OutputFormat,
) -> Result<Option<Scope>, ExitCode> {
    let Some(raw) = scope else {
        return Ok(None);
    };

    match serde_json::from_str::<Scope>(raw) {
        Ok(s) => Ok(Some(s)),
        Err(e) => Err(output_error(
            &crate::core::error::HivemindError::user(
                "invalid_scope",
                format!("Invalid scope definition: {e}"),
                "cli:task:create",
            ),
            format,
        )),
    }
}
pub(super) fn print_attempt_id(attempt_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            let info = serde_json::json!({"attempt_id": attempt_id});
            if let Ok(json) = serde_json::to_string_pretty(&info) {
                println!("{json}");
            }
        }
        OutputFormat::Yaml => {
            let info = serde_json::json!({"attempt_id": attempt_id});
            if let Ok(yaml) = serde_yaml::to_string(&info) {
                print!("{yaml}");
            }
        }
        OutputFormat::Table => {
            println!("Attempt ID: {attempt_id}");
        }
    }
}
