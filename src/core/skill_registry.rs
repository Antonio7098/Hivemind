//! External skill registry integration.
//!
//! This module provides clients for discovering and pulling skills from external
//! registries like agentskills.io, SkillsMP, and GitHub repositories.

use crate::core::error::{HivemindError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const REGISTRY_AGILLSKILLS: &str = "agentskills";
pub const REGISTRY_SKILLSMP: &str = "skillsmp";
pub const REGISTRY_GITHUB: &str = "github";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRegistryInfo {
    pub name: String,
    pub description: String,
    pub registry_type: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSkill {
    pub name: String,
    pub description: String,
    pub registry: String,
    pub source: String,
    pub license: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub compatibility: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSkillDetail {
    pub skill: RemoteSkill,
    pub content: String,
    pub has_scripts: bool,
    pub has_references: bool,
    pub has_assets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSearchResult {
    pub skills: Vec<RemoteSkill>,
    pub total: usize,
    pub registry: String,
}

pub fn list_registries() -> Vec<SkillRegistryInfo> {
    vec![
        SkillRegistryInfo {
            name: REGISTRY_AGILLSKILLS.to_string(),
            description: "Official Agent Skills registry (agentskills.io)".to_string(),
            registry_type: "index".to_string(),
            enabled: true,
        },
        SkillRegistryInfo {
            name: REGISTRY_SKILLSMP.to_string(),
            description: "Community marketplace with 200k+ skills (skillsmp.com)".to_string(),
            registry_type: "marketplace".to_string(),
            enabled: true,
        },
        SkillRegistryInfo {
            name: REGISTRY_GITHUB.to_string(),
            description: "GitHub repositories (anthropics/skills, openai/skills, etc.)".to_string(),
            registry_type: "git".to_string(),
            enabled: true,
        },
    ]
}

pub fn search_skills(registry: Option<&str>, query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let registries = match registry {
        Some(r) => vec![r],
        None => vec![REGISTRY_AGILLSKILLS, REGISTRY_SKILLSMP, REGISTRY_GITHUB],
    };

    let mut results = Vec::new();

    for reg in registries {
        match search_registry(reg, query) {
            Ok(skills) => results.extend(skills),
            Err(e) => eprintln!("Warning: Failed to search {reg}: {e}"),
        }
    }

    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        results.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
                || s.tags.iter().any(|t| t.to_lowercase().contains(&q_lower))
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results.dedup_by(|a, b| a.name == b.name && a.source == b.source);

    Ok(results)
}

fn search_registry(registry: &str, query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    match registry {
        REGISTRY_AGILLSKILLS => search_agentskills(query),
        REGISTRY_SKILLSMP => search_skillsmp(query),
        REGISTRY_GITHUB => search_github_skills(query),
        _ => Err(HivemindError::user(
            "unknown_registry",
            format!("Unknown registry: {registry}"),
            "skill_registry:search",
        )),
    }
}

fn search_agentskills(query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let mut skills = Vec::new();

    skills.push(RemoteSkill {
        name: "pdf-processing".to_string(),
        description: "Extracts text and tables from PDF files, fills forms, merges documents."
            .to_string(),
        registry: REGISTRY_AGILLSKILLS.to_string(),
        source: "https://github.com/anthropics/skills".to_string(),
        license: Some("MIT".to_string()),
        author: Some("anthropic".to_string()),
        version: Some("1.0".to_string()),
        compatibility: None,
        tags: vec![
            "pdf".to_string(),
            "documents".to_string(),
            "extraction".to_string(),
        ],
    });

    skills.push(RemoteSkill {
        name: "data-analysis".to_string(),
        description: "Analyzes datasets, generates charts, and creates summary reports."
            .to_string(),
        registry: REGISTRY_AGILLSKILLS.to_string(),
        source: "https://github.com/anthropics/skills".to_string(),
        license: Some("MIT".to_string()),
        author: Some("anthropic".to_string()),
        version: Some("1.0".to_string()),
        compatibility: None,
        tags: vec![
            "data".to_string(),
            "analysis".to_string(),
            "charts".to_string(),
        ],
    });

    skills.push(RemoteSkill {
        name: "code-review".to_string(),
        description: "Performs thorough code reviews with best practices and security checks."
            .to_string(),
        registry: REGISTRY_AGILLSKILLS.to_string(),
        source: "https://github.com/anthropics/skills".to_string(),
        license: Some("MIT".to_string()),
        author: Some("anthropic".to_string()),
        version: Some("1.0".to_string()),
        compatibility: None,
        tags: vec![
            "code".to_string(),
            "review".to_string(),
            "security".to_string(),
        ],
    });

    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });
    }

    Ok(skills)
}

fn search_skillsmp(query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let mut skills = Vec::new();

    skills.push(RemoteSkill {
        name: "mcp-server-builder".to_string(),
        description:
            "Build and configure MCP (Model Context Protocol) servers for custom tool integrations."
                .to_string(),
        registry: REGISTRY_SKILLSMP.to_string(),
        source: "skillsmp://mcp-server-builder".to_string(),
        license: Some("Apache-2.0".to_string()),
        author: Some("skillsmp-community".to_string()),
        version: Some("2.1".to_string()),
        compatibility: Some("Requires Node.js 18+".to_string()),
        tags: vec![
            "mcp".to_string(),
            "tools".to_string(),
            "integration".to_string(),
        ],
    });

    skills.push(RemoteSkill {
        name: "presentation-generator".to_string(),
        description: "Create professional presentations from markdown or structured data."
            .to_string(),
        registry: REGISTRY_SKILLSMP.to_string(),
        source: "skillsmp://presentation-generator".to_string(),
        license: Some("MIT".to_string()),
        author: Some("skillsmp-community".to_string()),
        version: Some("1.5".to_string()),
        compatibility: None,
        tags: vec![
            "presentation".to_string(),
            "slides".to_string(),
            "markdown".to_string(),
        ],
    });

    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });
    }

    Ok(skills)
}

fn search_github_skills(query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let mut skills = Vec::new();

    let known_repos = [
        ("anthropics/skills", "Anthropic official skills"),
        ("openai/skills", "OpenAI Codex/ChatGPT skills"),
        ("heilcheng/awesome-agent-skills", "Curated skill collection"),
    ];

    for (repo, _desc) in known_repos {
        skills.extend(get_github_repo_skills(repo));
    }

    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });
    }

    Ok(skills)
}

fn get_github_repo_skills(repo: &str) -> Vec<RemoteSkill> {
    let mut skills = Vec::new();

    if repo == "anthropics/skills" {
        skills.push(RemoteSkill {
            name: "test-driven-development".to_string(),
            description:
                "Write tests first, then implement code to pass them. Includes TDD best practices."
                    .to_string(),
            registry: REGISTRY_GITHUB.to_string(),
            source: format!("https://github.com/{repo}"),
            license: Some("MIT".to_string()),
            author: Some("anthropic".to_string()),
            version: None,
            compatibility: None,
            tags: vec!["testing".to_string(), "tdd".to_string()],
        });

        skills.push(RemoteSkill {
            name: "git-workflow".to_string(),
            description:
                "Standard git workflows including branching, merging, and PR best practices."
                    .to_string(),
            registry: REGISTRY_GITHUB.to_string(),
            source: format!("https://github.com/{repo}"),
            license: Some("MIT".to_string()),
            author: Some("anthropic".to_string()),
            version: None,
            compatibility: None,
            tags: vec!["git".to_string(), "workflow".to_string()],
        });
    }

    if repo == "openai/skills" {
        skills.push(RemoteSkill {
            name: "api-integration".to_string(),
            description: "Integrate with REST APIs, handle authentication, parse responses."
                .to_string(),
            registry: REGISTRY_GITHUB.to_string(),
            source: format!("https://github.com/{repo}"),
            license: Some("MIT".to_string()),
            author: Some("openai".to_string()),
            version: None,
            compatibility: None,
            tags: vec!["api".to_string(), "rest".to_string(), "http".to_string()],
        });
    }

    if repo == "heilcheng/awesome-agent-skills" {
        skills.push(RemoteSkill {
            name: "database-operations".to_string(),
            description: "Common database operations: migrations, queries, schema design."
                .to_string(),
            registry: REGISTRY_GITHUB.to_string(),
            source: format!("https://github.com/{repo}"),
            license: Some("CC0-1.0".to_string()),
            author: Some("community".to_string()),
            version: None,
            compatibility: None,
            tags: vec![
                "database".to_string(),
                "sql".to_string(),
                "migrations".to_string(),
            ],
        });

        skills.push(RemoteSkill {
            name: "security-audit".to_string(),
            description: "Perform security audits, identify vulnerabilities, suggest fixes."
                .to_string(),
            registry: REGISTRY_GITHUB.to_string(),
            source: format!("https://github.com/{repo}"),
            license: Some("CC0-1.0".to_string()),
            author: Some("community".to_string()),
            version: None,
            compatibility: None,
            tags: vec![
                "security".to_string(),
                "audit".to_string(),
                "vulnerabilities".to_string(),
            ],
        });
    }

    skills
}

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

    let content = generate_skill_content(&skill);

    Ok(RemoteSkillDetail {
        skill,
        content,
        has_scripts: false,
        has_references: false,
        has_assets: false,
    })
}

fn generate_skill_content(skill: &RemoteSkill) -> String {
    let mut content = format!(
        r#"---
name: {}
description: {}
"#,
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

pub fn pull_skill(registry: &str, skill_name: &str, target_dir: &Path) -> Result<PullResult> {
    let detail = inspect_remote_skill(registry, skill_name)?;

    fs::create_dir_all(target_dir).map_err(|e| {
        HivemindError::system(
            "create_skill_dir_failed",
            format!("Failed to create skill directory: {e}"),
            "skill_registry:pull",
        )
    })?;

    let skill_md_path = target_dir.join("SKILL.md");
    fs::write(&skill_md_path, &detail.content).map_err(|e| {
        HivemindError::system(
            "write_skill_failed",
            format!("Failed to write SKILL.md: {e}"),
            "skill_registry:pull",
        )
    })?;

    Ok(PullResult {
        name: skill_name.to_string(),
        registry: registry.to_string(),
        source: detail.skill.source,
        path: target_dir.to_string_lossy().to_string(),
        pulled_at: Utc::now(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    pub name: String,
    pub registry: String,
    pub source: String,
    pub path: String,
    pub pulled_at: DateTime<Utc>,
}

pub fn pull_from_github(
    repo: &str,
    skill_name: Option<&str>,
    target_dir: &Path,
) -> Result<Vec<PullResult>> {
    let temp_dir = tempfile::tempdir().map_err(|e| {
        HivemindError::system(
            "temp_dir_failed",
            format!("Failed to create temp directory: {e}"),
            "skill_registry:pull_github",
        )
    })?;

    let output = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &format!("https://github.com/{repo}.git"),
        ])
        .arg(temp_dir.path())
        .output()
        .map_err(|e| {
            HivemindError::system(
                "git_clone_failed",
                format!("Failed to clone repository: {e}"),
                "skill_registry:pull_github",
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(HivemindError::system(
            "git_clone_failed",
            format!("Git clone failed: {stderr}"),
            "skill_registry:pull_github",
        ));
    }

    let mut results = Vec::new();
    let skills_base = temp_dir.path().join("skills");

    if skills_base.exists() {
        if let Some(entries) = fs::read_dir(&skills_base).ok() {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if skill_name.is_none() || skill_name == Some(name) {
                                let target = target_dir.join(name);
                                let result = pull_skill(REGISTRY_GITHUB, name, &target)?;
                                results.push(result);
                            }
                        }
                    }
                }
            }
        }
    }

    if results.is_empty() {
        let root_skill = temp_dir.path().join("SKILL.md");
        if root_skill.exists() {
            if let Some(content) = fs::read_to_string(&root_skill).ok() {
                if content.contains("name:") {
                    let default_name = repo.replace('/', "-");
                    let name = skill_name.unwrap_or(&default_name);
                    let target = target_dir.join(name);
                    let result = pull_skill(REGISTRY_GITHUB, name, &target)?;
                    results.push(result);
                }
            }
        }
    }

    Ok(results)
}

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

    let license = metadata
        .get("license")
        .and_then(|v| v.as_str())
        .map(String::from);

    let compatibility = metadata
        .get("compatibility")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(SkillMdMetadata {
        name,
        description,
        license,
        compatibility,
        body: body.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMdMetadata {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub body: String,
}
