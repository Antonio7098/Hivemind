use super::*;
use crate::core::skill_registry::registries::search_registry;

pub fn inspect_remote_skill(registry: &str, skill_name: &str) -> Result<RemoteSkillDetail> {
    let skills = search_registry(registry, Some(skill_name))?;
    let skill = skills
        .iter()
        .find(|s| s.name == skill_name)
        .ok_or_else(|| {
            HivemindError::user(
                "skill_not_found",
                format!("Skill '{skill_name}' not found in registry '{registry}'"),
                "skill_registry:inspect",
            )
        })?
        .clone();

    Ok(RemoteSkillDetail {
        content: generate_skill_content(&skill),
        skill,
        has_scripts: false,
        has_references: false,
        has_assets: false,
    })
}

fn generate_skill_content(skill: &RemoteSkill) -> String {
    let mut content = format!(
        r"---
name: {}
description: {}
",
        skill.name, skill.description
    );

    if let Some(ref license) = skill.license {
        content.push_str(&format!("license: {license}\n"));
    }
    if let Some(ref compat) = skill.compatibility {
        content.push_str(&format!("compatibility: {compat}\n"));
    }
    content.push_str("---\n\n");
    content.push_str(&format!("# {}\n\n{}\n", skill.name, skill.description));

    if let Some(ref author) = skill.author {
        content.push_str(&format!("\n**Author:** {author}\n"));
    }
    if let Some(ref version) = skill.version {
        content.push_str(&format!("**Version:** {version}\n"));
    }
    if !skill.tags.is_empty() {
        content.push_str(&format!("\n**Tags:** {}\n", skill.tags.join(", ")));
    }

    content
}
