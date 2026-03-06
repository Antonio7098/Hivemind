//! Global command handlers.

use crate::cli::commands::{
    GlobalCommands, GlobalNotepadCommands, GlobalSkillCommands, GlobalSkillRegistryCommands,
    GlobalSystemPromptCommands, GlobalTemplateCommands,
};
use crate::cli::handlers::common::{get_registry, print_structured};
use crate::cli::output::{output, output_error, OutputFormat};
use crate::core::error::ExitCode;
use crate::core::registry::Registry;
use crate::core::skill_registry;

#[allow(clippy::too_many_lines)]
pub fn handle_global(cmd: GlobalCommands, format: OutputFormat) -> ExitCode {
    let Some(registry) = get_registry(format) else {
        return ExitCode::Error;
    };

    match cmd {
        GlobalCommands::Skill(cmd) => match cmd {
            GlobalSkillCommands::Create(args) => {
                match registry.global_skill_create(
                    &args.skill_id,
                    &args.name,
                    &args.tags,
                    &args.content,
                ) {
                    Ok(result) => {
                        print_structured(&result, format, "global skill create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSkillCommands::List => match registry.global_skill_list() {
                Ok(result) => {
                    print_structured(&result, format, "global skill list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Inspect(args) => {
                match registry.global_skill_inspect(&args.skill_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global skill inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSkillCommands::Update(args) => match registry.global_skill_update(
                &args.skill_id,
                args.name.as_deref(),
                args.tags.as_deref(),
                args.content.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global skill update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Delete(args) => match registry.global_skill_delete(&args.skill_id)
            {
                Ok(result) => {
                    print_structured(&result, format, "global skill delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSkillCommands::Registry(cmd) => handle_skill_registry(cmd, &registry, format),
        },
        GlobalCommands::SystemPrompt(cmd) => match cmd {
            GlobalSystemPromptCommands::Create(args) => {
                match registry.global_system_prompt_create(&args.prompt_id, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::List => match registry.global_system_prompt_list() {
                Ok(result) => {
                    print_structured(&result, format, "global system prompt list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalSystemPromptCommands::Inspect(args) => {
                match registry.global_system_prompt_inspect(&args.prompt_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::Update(args) => {
                match registry.global_system_prompt_update(&args.prompt_id, &args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalSystemPromptCommands::Delete(args) => {
                match registry.global_system_prompt_delete(&args.prompt_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global system prompt delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        GlobalCommands::Template(cmd) => match cmd {
            GlobalTemplateCommands::Create(args) => match registry.global_template_create(
                &args.template_id,
                &args.system_prompt_id,
                &args.skill_ids,
                &args.document_ids,
                args.description.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global template create result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::List => match registry.global_template_list() {
                Ok(result) => {
                    print_structured(&result, format, "global template list");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::Inspect(args) => {
                match registry.global_template_inspect(&args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template inspect result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalTemplateCommands::Update(args) => match registry.global_template_update(
                &args.template_id,
                args.system_prompt_id.as_deref(),
                args.skill_ids.as_deref(),
                args.document_ids.as_deref(),
                args.description.as_deref(),
            ) {
                Ok(result) => {
                    print_structured(&result, format, "global template update result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalTemplateCommands::Delete(args) => {
                match registry.global_template_delete(&args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template delete result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalTemplateCommands::Instantiate(args) => {
                match registry.global_template_instantiate(&args.project, &args.template_id) {
                    Ok(result) => {
                        print_structured(&result, format, "global template instantiate result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
        },
        GlobalCommands::Notepad(cmd) => match cmd {
            GlobalNotepadCommands::Create(args) => {
                match registry.global_notepad_create(&args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global notepad create result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalNotepadCommands::Show => match registry.global_notepad_show() {
                Ok(result) => {
                    print_structured(&result, format, "global notepad show result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
            GlobalNotepadCommands::Update(args) => {
                match registry.global_notepad_update(&args.content) {
                    Ok(result) => {
                        print_structured(&result, format, "global notepad update result");
                        ExitCode::Success
                    }
                    Err(e) => output_error(&e, format),
                }
            }
            GlobalNotepadCommands::Delete => match registry.global_notepad_delete() {
                Ok(result) => {
                    print_structured(&result, format, "global notepad delete result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            },
        },
    }
}

#[allow(clippy::too_many_lines)]
fn handle_skill_registry(
    cmd: GlobalSkillRegistryCommands,
    registry: &Registry,
    format: OutputFormat,
) -> ExitCode {
    match cmd {
        GlobalSkillRegistryCommands::RegistryList => {
            let registries = skill_registry::list_registries();
            match format {
                OutputFormat::Table => {
                    println!("{:<15}  {:<12}  DESCRIPTION", "NAME", "TYPE");
                    println!("{}", "-".repeat(80));
                    for r in &registries {
                        println!("{:<15}  {:<12}  {}", r.name, r.registry_type, r.description);
                    }
                }
                _ => {
                    if let Err(err) = output(&registries, format) {
                        eprintln!("Failed to render registries: {err}");
                    }
                }
            }
            ExitCode::Success
        }
        GlobalSkillRegistryCommands::Search(args) => {
            match skill_registry::search_skills(args.registry.as_deref(), args.query.as_deref()) {
                Ok(skills) => match format {
                    OutputFormat::Table => {
                        if skills.is_empty() {
                            println!("No skills found.");
                        } else {
                            println!("{:<25}  {:<12}  DESCRIPTION", "NAME", "REGISTRY");
                            println!("{}", "-".repeat(90));
                            for s in &skills {
                                let desc = if s.description.len() > 50 {
                                    format!("{}...", &s.description[..47])
                                } else {
                                    s.description.clone()
                                };
                                println!("{:<25}  {:<12}  {}", s.name, s.registry, desc);
                            }
                        }
                    }
                    _ => {
                        if let Err(err) = output(&skills, format) {
                            eprintln!("Failed to render skills: {err}");
                        }
                    }
                },
                Err(e) => return output_error(&e, format),
            }
            ExitCode::Success
        }
        GlobalSkillRegistryCommands::RegistryInspect(args) => {
            match skill_registry::inspect_remote_skill(&args.registry, &args.skill_name) {
                Ok(detail) => {
                    print_structured(&detail, format, "remote skill inspect result");
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSkillRegistryCommands::Pull(args) => {
            let skill_id = args.skill_id.as_deref().unwrap_or(&args.skill_name);
            let detail =
                match skill_registry::inspect_remote_skill(&args.registry, &args.skill_name) {
                    Ok(d) => d,
                    Err(e) => return output_error(&e, format),
                };

            match registry.global_skill_create(
                skill_id,
                &detail.skill.name,
                &detail.skill.tags,
                &detail.content,
            ) {
                Ok(result) => {
                    match format {
                        OutputFormat::Table => {
                            println!(
                                "Pulled skill '{}' from {} and saved as '{}'",
                                detail.skill.name, args.registry, skill_id
                            );
                        }
                        _ => {
                            print_structured(&result, format, "skill pull result");
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
        GlobalSkillRegistryCommands::PullGithub(args) => {
            let global_root = registry.governance_global_root();
            let skills_dir = global_root.join("skills");

            match skill_registry::pull_from_github(&args.repo, args.skill.as_deref(), &skills_dir) {
                Ok(results) => {
                    match format {
                        OutputFormat::Table => {
                            if results.is_empty() {
                                println!("No skills found in repository.");
                            } else {
                                println!("Pulled {} skill(s) from {}:", results.len(), args.repo);
                                for r in &results {
                                    println!("  - {} -> {}", r.name, r.path);
                                }
                            }
                        }
                        _ => {
                            print_structured(&results, format, "github pull result");
                        }
                    }
                    ExitCode::Success
                }
                Err(e) => output_error(&e, format),
            }
        }
    }
}

pub fn handle_global_skill(cmd: GlobalSkillCommands, format: OutputFormat) -> ExitCode {
    handle_global(GlobalCommands::Skill(cmd), format)
}

pub fn handle_global_system_prompt(
    cmd: GlobalSystemPromptCommands,
    format: OutputFormat,
) -> ExitCode {
    handle_global(GlobalCommands::SystemPrompt(cmd), format)
}

pub fn handle_global_template(cmd: GlobalTemplateCommands, format: OutputFormat) -> ExitCode {
    handle_global(GlobalCommands::Template(cmd), format)
}

pub fn handle_global_notepad(cmd: GlobalNotepadCommands, format: OutputFormat) -> ExitCode {
    handle_global(GlobalCommands::Notepad(cmd), format)
}

pub fn handle_global_skill_registry(
    cmd: GlobalSkillRegistryCommands,
    format: OutputFormat,
) -> ExitCode {
    handle_global(
        GlobalCommands::Skill(GlobalSkillCommands::Registry(cmd)),
        format,
    )
}
