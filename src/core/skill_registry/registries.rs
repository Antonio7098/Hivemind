use super::*;

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
    #[allow(clippy::option_if_let_else)]
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

pub(super) fn search_registry(registry: &str, query: Option<&str>) -> Result<Vec<RemoteSkill>> {
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
    let mut skills = vec![
        RemoteSkill {
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
        },
        RemoteSkill {
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
        },
        RemoteSkill {
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
        },
    ];
    filter_by_name_or_description(&mut skills, query);
    Ok(skills)
}

fn search_skillsmp(query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let mut skills = vec![
        RemoteSkill {
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
            tags: vec!["mcp".to_string(), "tools".to_string(), "integration".to_string()],
        },
        RemoteSkill {
            name: "presentation-generator".to_string(),
            description: "Create professional presentations from markdown or structured data."
                .to_string(),
            registry: REGISTRY_SKILLSMP.to_string(),
            source: "skillsmp://presentation-generator".to_string(),
            license: Some("MIT".to_string()),
            author: Some("skillsmp-community".to_string()),
            version: Some("1.5".to_string()),
            compatibility: None,
            tags: vec!["presentation".to_string(), "slides".to_string(), "markdown".to_string()],
        },
    ];
    filter_by_name_or_description(&mut skills, query);
    Ok(skills)
}

fn search_github_skills(query: Option<&str>) -> Result<Vec<RemoteSkill>> {
    let known_repos = [
        ("anthropics/skills", "Anthropic official skills"),
        ("openai/skills", "OpenAI Codex/ChatGPT skills"),
        ("heilcheng/awesome-agent-skills", "Curated skill collection"),
    ];

    let mut skills = Vec::new();
    for (repo, _desc) in known_repos {
        skills.extend(get_github_repo_skills(repo));
    }
    filter_by_name_or_description(&mut skills, query);
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

fn filter_by_name_or_description(skills: &mut Vec<RemoteSkill>, query: Option<&str>) {
    if let Some(q) = query {
        let q_lower = q.to_lowercase();
        skills.retain(|s| {
            s.name.to_lowercase().contains(&q_lower)
                || s.description.to_lowercase().contains(&q_lower)
        });
    }
}
