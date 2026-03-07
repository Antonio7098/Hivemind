use super::*;

pub(super) fn print_project(project: &Project, format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            println!("ID:          {}", project.id);
            println!("Name:        {}", project.name);
            if let Some(desc) = &project.description {
                println!("Description: {desc}");
            }
            println!("Created:     {}", project.created_at);
            println!("Updated:     {}", project.updated_at);
        }
        _ => {
            if let Err(err) = output(project, format) {
                eprintln!("Failed to render project: {err}");
            }
        }
    }
}

pub(super) fn print_project_id(project_id: Uuid, format: OutputFormat) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({"project_id": project_id}));
        }
        OutputFormat::Table => {
            println!("Project ID: {project_id}");
        }
        OutputFormat::Yaml => {
            if let Ok(yaml) =
                serde_yaml::to_string(&serde_json::json!({"project_id": project_id.to_string()}))
            {
                print!("{yaml}");
            }
        }
    }
}

pub(super) fn print_projects(projects: &[Project], format: OutputFormat) {
    match format {
        OutputFormat::Table => {
            if projects.is_empty() {
                println!("No projects found.");
                return;
            }
            println!("{:<36}  {:<24}  DESCRIPTION", "ID", "NAME");
            println!("{}", "-".repeat(90));
            for p in projects {
                println!(
                    "{:<36}  {:<24}  {}",
                    p.id,
                    p.name,
                    p.description.clone().unwrap_or_default()
                );
            }
        }
        _ => {
            if let Err(err) = output(projects, format) {
                eprintln!("Failed to render projects: {err}");
            }
        }
    }
}

pub(super) fn print_project_governance_init(
    result: &ProjectGovernanceInitResult,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(result) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance init result: {err}");
            }
        }
    }
}

pub(super) fn print_project_governance_migrate(
    result: &ProjectGovernanceMigrateResult,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(result) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance migrate result: {err}");
            }
        }
    }
}

pub(super) fn print_project_governance_inspect(
    result: &ProjectGovernanceInspectResult,
    format: OutputFormat,
) {
    match format {
        OutputFormat::Table => {
            if let Ok(json) = serde_json::to_string_pretty(result) {
                println!("{json}");
            }
        }
        _ => {
            if let Err(err) = output(result, format) {
                eprintln!("Failed to render governance inspect result: {err}");
            }
        }
    }
}
