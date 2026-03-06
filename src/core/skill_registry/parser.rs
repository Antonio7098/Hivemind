use super::*;
use std::collections::HashMap;

pub fn parse_skill_md(content: &str) -> Result<SkillMdMetadata> {
    let content = content.trim();
    if !content.starts_with("---") {
        return Err(HivemindError::user(
            "invalid_skill_format",
            "SKILL.md must start with YAML frontmatter",
            "skill_registry:parse",
        ));
    }

    let end_idx = content[3..].find("\n---").ok_or_else(|| {
        HivemindError::user(
            "invalid_skill_format",
            "SKILL.md frontmatter must be terminated with ---",
            "skill_registry:parse",
        )
    })? + 3;

    let frontmatter = &content[4..end_idx];
    let body = &content[end_idx + 5..];
    let metadata: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_str(frontmatter).map_err(|e| {
            HivemindError::user(
                "invalid_skill_frontmatter",
                format!("Invalid YAML frontmatter: {e}"),
                "skill_registry:parse",
            )
        })?;

    let name = metadata
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            HivemindError::user(
                "missing_skill_name",
                "SKILL.md frontmatter must include 'name' field",
                "skill_registry:parse",
            )
        })?
        .to_string();
    let description = metadata
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            HivemindError::user(
                "missing_skill_description",
                "SKILL.md frontmatter must include 'description' field",
                "skill_registry:parse",
            )
        })?
        .to_string();

    Ok(SkillMdMetadata {
        name,
        description,
        license: metadata
            .get("license")
            .and_then(|v| v.as_str())
            .map(String::from),
        compatibility: metadata
            .get("compatibility")
            .and_then(|v| v.as_str())
            .map(String::from),
        body: body.to_string(),
    })
}
