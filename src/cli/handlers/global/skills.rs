use super::*;

#[allow(clippy::too_many_lines)]
pub(super) fn handle_skill_registry(
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
pub fn handle_global_skill_registry(
    cmd: GlobalSkillRegistryCommands,
    format: OutputFormat,
) -> ExitCode {
    handle_global(
        GlobalCommands::Skill(GlobalSkillCommands::Registry(cmd)),
        format,
    )
}
