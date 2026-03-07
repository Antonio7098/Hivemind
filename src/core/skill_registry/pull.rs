use super::*;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn pull_skill(registry: &str, skill_name: &str, target_dir: &Path) -> Result<PullResult> {
    let detail = super::inspect_remote_skill(registry, skill_name)?;

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
        if let Ok(entries) = fs::read_dir(&skills_base) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skill_md = path.join("SKILL.md");
                    if skill_md.exists() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if skill_name.is_none() || skill_name == Some(name) {
                                results.push(pull_skill(
                                    REGISTRY_GITHUB,
                                    name,
                                    &target_dir.join(name),
                                )?);
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
            if let Ok(content) = fs::read_to_string(&root_skill) {
                if content.contains("name:") {
                    let default_name = repo.replace('/', "-");
                    let name = skill_name.unwrap_or(&default_name);
                    results.push(pull_skill(REGISTRY_GITHUB, name, &target_dir.join(name))?);
                }
            }
        }
    }

    Ok(results)
}
