use super::*;

#[test]
fn search_skills_filters_across_registries() {
    let results = search_skills(None, Some("security")).expect("search should succeed");
    assert!(!results.is_empty());
    assert!(results.iter().any(|s| s.name == "security-audit"));
}

#[test]
fn inspect_remote_skill_generates_frontmatter_content() {
    let detail =
        inspect_remote_skill(REGISTRY_AGILLSKILLS, "code-review").expect("inspect should succeed");
    assert_eq!(detail.skill.name, "code-review");
    assert!(detail.content.starts_with("---\nname: code-review"));
    assert!(detail.content.contains("description:"));
}

#[test]
fn pull_skill_writes_skill_md() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("skill");
    let result = pull_skill(REGISTRY_GITHUB, "git-workflow", &target).expect("pull should succeed");

    assert_eq!(result.name, "git-workflow");
    let written = std::fs::read_to_string(target.join("SKILL.md")).expect("skill file");
    assert!(written.contains("git-workflow"));
}

#[test]
fn parse_skill_md_extracts_metadata_and_body() {
    let raw = r#"---
name: sample
description: Sample skill
license: MIT
compatibility: Rust
---

hello body
"#;
    let parsed = parse_skill_md(raw).expect("parse should succeed");
    assert_eq!(parsed.name, "sample");
    assert_eq!(parsed.description, "Sample skill");
    assert_eq!(parsed.license.as_deref(), Some("MIT"));
    assert_eq!(parsed.compatibility.as_deref(), Some("Rust"));
    assert!(parsed.body.contains("hello body"));
}

#[test]
fn parse_skill_md_rejects_missing_frontmatter() {
    let err = parse_skill_md("# missing frontmatter").expect_err("parse should fail");
    assert_eq!(err.code, "invalid_skill_format");
}
