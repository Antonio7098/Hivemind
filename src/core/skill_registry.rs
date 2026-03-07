//! External skill registry integration.
//!
//! This module provides clients for discovering and pulling skills from external
//! registries like agentskills.io, `SkillsMP`, and GitHub repositories.

#![allow(clippy::unnecessary_wraps)]
#![allow(clippy::format_push_string)]

use crate::core::error::{HivemindError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod content;
mod parser;
mod pull;
mod registries;

#[cfg(test)]
mod tests;

pub use content::inspect_remote_skill;
pub use parser::parse_skill_md;
pub use pull::{pull_from_github, pull_skill};
pub use registries::{list_registries, search_skills};

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
pub struct PullResult {
    pub name: String,
    pub registry: String,
    pub source: String,
    pub path: String,
    pub pulled_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMdMetadata {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub body: String,
}
