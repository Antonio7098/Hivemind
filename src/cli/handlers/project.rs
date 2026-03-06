//! Project command handlers.

use crate::cli::commands::{
    ProjectCommands, ProjectGovernanceAttachmentCommands, ProjectGovernanceCommands,
    ProjectGovernanceDocumentCommands, ProjectGovernanceNotepadCommands,
    ProjectGovernanceRepairCommands, ProjectGovernanceSnapshotCommands,
};
use crate::cli::handlers::common::{get_registry, parse_runtime_role, print_structured};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::{ExitCode, HivemindError};
use crate::core::registry::{
    ProjectGovernanceInitResult, ProjectGovernanceInspectResult, ProjectGovernanceMigrateResult,
};
use crate::core::scope::RepoAccessMode;
use crate::core::state::Project;
use uuid::Uuid;

fn print_project(project: &Project, format: OutputFormat) {
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

fn print_project_id(project_id: Uuid, format: OutputFormat) {
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

fn print_projects(projects: &[Project], format: OutputFormat) {
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

fn print_project_governance_init(result: &ProjectGovernanceInitResult, format: OutputFormat) {
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

fn print_project_governance_migrate(result: &ProjectGovernanceMigrateResult, format: OutputFormat) {
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

fn print_project_governance_inspect(result: &ProjectGovernanceInspectResult, format: OutputFormat) {
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

fn resolve_required_selector(
    positional: Option<&str>,
    flag_value: Option<&str>,
    flag_name: &str,
    noun: &str,
    origin: &str,
) -> Result<String, HivemindError> {
    let pos = positional.map(str::trim).filter(|s| !s.is_empty());
    let flag = flag_value.map(str::trim).filter(|s| !s.is_empty());
    match (pos, flag) {
        (Some(a), Some(b)) if a != b => Err(HivemindError::user(
            "selector_conflict",
            format!("Conflicting {noun} values provided via positional argument and {flag_name}"),
            origin,
        )
        .with_context("positional", a)
        .with_context("flag", b)),
        (Some(a), _) => Ok(a.to_string()),
        (None, Some(b)) => Ok(b.to_string()),
        (None, None) => Err(HivemindError::user(
            "missing_required_selector",
            format!("Provide {noun} as a positional argument or via {flag_name}"),
            origin,
        )),
    }
}

#[allow(clippy::too_many_lines)]
pub fn handle_project(cmd: ProjectCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        ProjectCommands::Create(args) => {
            match registry.create_project(&args.name, args.description.as_deref()) {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::List => match registry.list_projects() {
            Ok(projects) => {
                print_projects(&projects, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Inspect(args) => match registry.get_project(&args.project) {
            Ok(project) => {
                print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Update(args) => match registry.update_project(
            &args.project,
            args.name.as_deref(),
            args.description.as_deref(),
        ) {
            Ok(project) => {
                print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::RuntimeSet(args) => match registry.project_runtime_set_role(
            &args.project,
            parse_runtime_role(args.role),
            &args.adapter,
            &args.binary_path,
            args.model,
            &args.args,
            &args.env,
            args.timeout_ms,
            args.max_parallel_tasks,
        ) {
            Ok(project) => {
                print_project(&project, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::AttachRepo(args) => {
            let access_mode = match args.access.to_lowercase().as_str() {
                "ro" => RepoAccessMode::ReadOnly,
                "rw" => RepoAccessMode::ReadWrite,
                other => {
                    return output_error(
                        &HivemindError::user(
                            "invalid_access_mode",
                            format!("Invalid access mode: '{other}'"),
                            "cli:project:attach-repo",
                        )
                        .with_hint("Use --access ro or --access rw"),
                        format,
                    );
                }
            };

            match registry.attach_repo(&args.project, &args.path, args.name.as_deref(), access_mode)
            {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::DetachRepo(args) => {
            match registry.detach_repo(&args.project, &args.repo_name) {
                Ok(project) => {
                    print_project(&project, format);
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        ProjectCommands::Delete(args) => match registry.delete_project(&args.project) {
            Ok(project_id) => {
                print_project_id(project_id, format);
                ExitCode::Success
            }
            Err(e) => output_error(&e, format),
        },
        ProjectCommands::Governance(cmd) => match cmd {
            ProjectGovernanceCommands::Init(args) => {
                let project = match resolve_required_selector(
                    args.project.as_deref(),
                    args.project_flag.as_deref(),
                    "--project",
                    "project",
                    "cli:project:governance:init",
                ) {
                    Ok(project) => project,
                    Err(e) => return output_error(&e, format),
                };
                match registry.project_governance_init(&project) {
                    Ok(result) => {
                        print_project_governance_init(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Migrate(args) => {
                match registry.project_governance_migrate(&args.project) {
                    Ok(result) => {
                        print_project_governance_migrate(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Inspect(args) => {
                match registry.project_governance_inspect(&args.project) {
                    Ok(result) => {
                        print_project_governance_inspect(&result, format);
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Diagnose(args) => {
                match registry.project_governance_diagnose(&args.project) {
                    Ok(result) => {
                        print_structured(&result, format, "project governance diagnostics");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Replay(args) => {
                match registry.project_governance_replay(&args.project, args.verify) {
                    Ok(result) => {
                        print_structured(&result, format, "project governance replay result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            ProjectGovernanceCommands::Snapshot(cmd) => match cmd {
                ProjectGovernanceSnapshotCommands::Create(args) => match registry
                    .project_governance_snapshot_create(&args.project, args.interval_minutes)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceSnapshotCommands::List(args) => {
                    match registry.project_governance_snapshot_list(&args.project, args.limit) {
                        Ok(result) => {
                            print_structured(&result, format, "governance snapshot list result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceSnapshotCommands::Restore(args) => match registry
                    .project_governance_snapshot_restore(
                        &args.project,
                        &args.snapshot_id,
                        args.confirm,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance snapshot restore result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Repair(cmd) => match cmd {
                ProjectGovernanceRepairCommands::Detect(args) => {
                    match registry.project_governance_repair_detect(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "governance repair detect result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceRepairCommands::Preview(args) => match registry
                    .project_governance_repair_preview(&args.project, args.snapshot_id.as_deref())
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair preview result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceRepairCommands::Apply(args) => match registry
                    .project_governance_repair_apply(
                        &args.project,
                        args.snapshot_id.as_deref(),
                        args.confirm,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance repair apply result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Document(cmd) => match cmd {
                ProjectGovernanceDocumentCommands::Create(args) => match registry
                    .project_governance_document_create(
                        &args.project,
                        &args.document_id,
                        &args.title,
                        &args.owner,
                        &args.tags,
                        &args.content,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::List(args) => {
                    match registry.project_governance_document_list(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "governance document list");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceDocumentCommands::Inspect(args) => match registry
                    .project_governance_document_inspect(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::Update(args) => match registry
                    .project_governance_document_update(
                        &args.project,
                        &args.document_id,
                        args.title.as_deref(),
                        args.owner.as_deref(),
                        args.tags.as_deref(),
                        args.content.as_deref(),
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance document update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceDocumentCommands::Delete(args) => match registry
                    .project_governance_document_delete(&args.project, &args.document_id)
                {
                    Ok(result) => {
                        print_structured(&result, format, "governance document delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Attachment(cmd) => match cmd {
                ProjectGovernanceAttachmentCommands::Include(args) => match registry
                    .project_governance_attachment_set_document(
                        &args.project,
                        &args.task_id,
                        &args.document_id,
                        true,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment include result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
                ProjectGovernanceAttachmentCommands::Exclude(args) => match registry
                    .project_governance_attachment_set_document(
                        &args.project,
                        &args.task_id,
                        &args.document_id,
                        false,
                    ) {
                    Ok(result) => {
                        print_structured(&result, format, "governance attachment exclude result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                },
            },
            ProjectGovernanceCommands::Notepad(cmd) => match cmd {
                ProjectGovernanceNotepadCommands::Create(args) => {
                    match registry.project_governance_notepad_create(&args.project, &args.content) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad create result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Show(args) => {
                    match registry.project_governance_notepad_show(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad show result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Update(args) => {
                    match registry.project_governance_notepad_update(&args.project, &args.content) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad update result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
                ProjectGovernanceNotepadCommands::Delete(args) => {
                    match registry.project_governance_notepad_delete(&args.project) {
                        Ok(result) => {
                            print_structured(&result, format, "project notepad delete result");
                            ExitCode::Success
                        }
                        Err(e) => output_error(&e, format),
                    }
                }
            },
        },
    }
}
