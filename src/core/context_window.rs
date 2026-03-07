//! Attempt-scoped context window with deterministic operation semantics.
//!
//! This module is adapted from the UCP context management model and tuned for
//! Hivemind's prompt assembly pipeline.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet};

mod mutation;
mod pruning;
mod snapshot;
mod support;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextOperationActor {
    pub actor: String,
    #[serde(default)]
    pub runtime: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudgetPolicy {
    pub total_budget_bytes: usize,
    pub default_section_budget_bytes: usize,
    #[serde(default)]
    pub per_section_budget_bytes: BTreeMap<String, usize>,
    pub max_expand_depth: usize,
    pub deduplicate: bool,
    pub truncation_policy: String,
}

impl ContextBudgetPolicy {
    #[must_use]
    pub fn section_budget(&self, section: &str) -> usize {
        self.per_section_budget_bytes
            .get(section)
            .copied()
            .unwrap_or(self.default_section_budget_bytes)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntryCandidate {
    pub entry_id: String,
    pub section: String,
    pub content: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextEntry {
    pub entry_id: String,
    pub section: String,
    pub content: String,
    pub source: String,
    pub depth: usize,
    pub added_sequence: u64,
}

impl ContextEntry {
    #[must_use]
    pub fn size_bytes(&self) -> usize {
        self.content.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextOpRecord {
    pub sequence: u64,
    pub op: String,
    pub reason: String,
    pub actor: ContextOperationActor,
    pub before_hash: String,
    pub after_hash: String,
    #[serde(default)]
    pub added_ids: Vec<String>,
    #[serde(default)]
    pub removed_ids: Vec<String>,
    #[serde(default)]
    pub truncated_sections: Vec<String>,
    #[serde(default)]
    pub section_reasons: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedSection {
    pub section: String,
    pub content: String,
    pub size_bytes: usize,
    #[serde(default)]
    pub entry_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindowSnapshot {
    pub window_id: String,
    pub state_hash: String,
    pub rendered_prompt_hash: String,
    pub total_size_bytes: usize,
    #[serde(default)]
    pub sections: Vec<RenderedSection>,
    pub rendered_prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextWindow {
    pub window_id: String,
    #[serde(default)]
    pub ordered_sections: Vec<String>,
    pub budget_policy: ContextBudgetPolicy,
    #[serde(default)]
    pub entries: BTreeMap<String, ContextEntry>,
    #[serde(default)]
    pub operations: Vec<ContextOpRecord>,
    pub next_sequence: u64,
}

impl ContextWindow {
    #[must_use]
    pub fn new(
        window_id: impl Into<String>,
        ordered_sections: Vec<String>,
        budget_policy: ContextBudgetPolicy,
    ) -> Self {
        Self {
            window_id: window_id.into(),
            ordered_sections,
            budget_policy,
            entries: BTreeMap::new(),
            operations: Vec::new(),
            next_sequence: 1,
        }
    }
}
