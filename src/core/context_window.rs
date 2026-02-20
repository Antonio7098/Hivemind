//! Attempt-scoped context window with deterministic operation semantics.
//!
//! This module is adapted from the UCP context management model and tuned for
//! Hivemind's prompt assembly pipeline.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet};

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

    #[must_use]
    pub fn state_hash(&self) -> String {
        let payload = serde_json::json!({
            "ordered_sections": self.ordered_sections,
            "budget_policy": self.budget_policy,
            "entries": self.entries,
        });
        let bytes = serde_json::to_vec(&payload).unwrap_or_default();
        digest_hex(&bytes)
    }

    pub fn add_entry(
        &mut self,
        candidate: ContextEntryCandidate,
        reason: impl Into<String>,
        actor: ContextOperationActor,
    ) -> ContextOpRecord {
        self.expand_entries(vec![candidate], reason, actor, false)
    }

    pub fn expand_entries(
        &mut self,
        mut candidates: Vec<ContextEntryCandidate>,
        reason: impl Into<String>,
        actor: ContextOperationActor,
        is_expand: bool,
    ) -> ContextOpRecord {
        candidates.sort_by(|a, b| a.entry_id.cmp(&b.entry_id));
        let before_hash = self.state_hash();
        let mut added_ids = Vec::new();

        for candidate in candidates {
            if candidate.depth > self.budget_policy.max_expand_depth {
                continue;
            }
            if self.budget_policy.deduplicate
                && self.has_duplicate(&candidate.section, &candidate.content)
            {
                continue;
            }
            if self.entries.contains_key(&candidate.entry_id) {
                continue;
            }
            let entry = ContextEntry {
                entry_id: candidate.entry_id.clone(),
                section: candidate.section,
                content: candidate.content,
                source: candidate.source,
                depth: candidate.depth,
                added_sequence: self.take_sequence(),
            };
            self.entries.insert(entry.entry_id.clone(), entry);
            added_ids.push(candidate.entry_id);
        }

        let op_sequence = self.take_sequence();
        let op = self.record_op(
            op_sequence,
            if is_expand { "expand" } else { "add" },
            reason.into(),
            actor,
            before_hash,
            added_ids,
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        self.operations.push(op.clone());
        op
    }

    pub fn remove_entry(
        &mut self,
        entry_id: &str,
        reason: impl Into<String>,
        actor: ContextOperationActor,
    ) -> ContextOpRecord {
        let before_hash = self.state_hash();
        let mut removed_ids = Vec::new();
        if self.entries.remove(entry_id).is_some() {
            removed_ids.push(entry_id.to_string());
        }
        let op_sequence = self.take_sequence();
        let op = self.record_op(
            op_sequence,
            "remove",
            reason.into(),
            actor,
            before_hash,
            Vec::new(),
            removed_ids,
            Vec::new(),
            BTreeMap::new(),
        );
        self.operations.push(op.clone());
        op
    }

    #[allow(clippy::too_many_lines)]
    pub fn prune(
        &mut self,
        reason: impl Into<String>,
        actor: ContextOperationActor,
    ) -> ContextOpRecord {
        let before_hash = self.state_hash();
        let mut to_remove: BTreeSet<String> = BTreeSet::new();
        let mut section_reasons: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        // Enforce max depth first.
        for entry in self.entries.values() {
            if entry.depth > self.budget_policy.max_expand_depth {
                to_remove.insert(entry.entry_id.clone());
                section_reasons
                    .entry(entry.section.clone())
                    .or_default()
                    .insert("max_expand_depth_exceeded".to_string());
            }
        }

        // Enforce de-duplication per section.
        if self.budget_policy.deduplicate {
            for section in self.section_order() {
                let mut seen = HashSet::new();
                for entry in self.section_entries(&section) {
                    if to_remove.contains(&entry.entry_id) {
                        continue;
                    }
                    let key = digest_hex(entry.content.as_bytes());
                    if !seen.insert(key) {
                        to_remove.insert(entry.entry_id.clone());
                        section_reasons
                            .entry(section.clone())
                            .or_default()
                            .insert("deduplicated_content".to_string());
                    }
                }
            }
        }

        // Enforce section-level budgets.
        for section in self.section_order() {
            let mut used = 0usize;
            let budget = self.budget_policy.section_budget(&section);
            for entry in self.section_entries(&section) {
                if to_remove.contains(&entry.entry_id) {
                    continue;
                }
                let size = entry.size_bytes();
                if used == 0 && size > budget {
                    used = used.saturating_add(size);
                    continue;
                }
                if used.saturating_add(size) > budget {
                    to_remove.insert(entry.entry_id.clone());
                    section_reasons
                        .entry(section.clone())
                        .or_default()
                        .insert("section_budget_exceeded".to_string());
                    continue;
                }
                used = used.saturating_add(size);
            }
        }

        // Enforce total budget across ordered sections.
        let mut total_used = 0usize;
        for section in self.section_order() {
            for entry in self.section_entries(&section) {
                if to_remove.contains(&entry.entry_id) {
                    continue;
                }
                let size = entry.size_bytes();
                if total_used == 0 && size > self.budget_policy.total_budget_bytes {
                    total_used = total_used.saturating_add(size);
                    continue;
                }
                if total_used.saturating_add(size) > self.budget_policy.total_budget_bytes {
                    to_remove.insert(entry.entry_id.clone());
                    section_reasons
                        .entry(section.clone())
                        .or_default()
                        .insert("total_budget_exceeded".to_string());
                    continue;
                }
                total_used = total_used.saturating_add(size);
            }
        }

        let removed_ids: Vec<String> = to_remove.iter().cloned().collect();
        for id in &removed_ids {
            self.entries.remove(id);
        }

        let mut truncated_sections: Vec<String> = section_reasons
            .iter()
            .filter_map(|(section, reasons)| {
                if reasons.contains("section_budget_exceeded")
                    || reasons.contains("total_budget_exceeded")
                {
                    Some(section.clone())
                } else {
                    None
                }
            })
            .collect();
        truncated_sections.sort();

        let section_reasons: BTreeMap<String, Vec<String>> = section_reasons
            .into_iter()
            .map(|(section, reasons)| (section, reasons.into_iter().collect()))
            .collect();

        let op_sequence = self.take_sequence();
        let op = self.record_op(
            op_sequence,
            "prune",
            reason.into(),
            actor,
            before_hash,
            Vec::new(),
            removed_ids,
            truncated_sections,
            section_reasons,
        );
        self.operations.push(op.clone());
        op
    }

    pub fn snapshot(
        &mut self,
        reason: impl Into<String>,
        actor: ContextOperationActor,
    ) -> (ContextWindowSnapshot, ContextOpRecord) {
        let before_hash = self.state_hash();
        let mut sections = Vec::new();
        let mut rendered_parts = Vec::new();
        let mut total_size_bytes = 0usize;

        for section in self.section_order() {
            let entries = self.section_entries(&section);
            let content = entries
                .iter()
                .map(|entry| entry.content.clone())
                .collect::<Vec<_>>()
                .join("\n\n");
            let size_bytes = content.len();
            total_size_bytes = total_size_bytes.saturating_add(size_bytes);
            let entry_ids = entries
                .into_iter()
                .map(|entry| entry.entry_id.clone())
                .collect();
            if !content.is_empty() {
                rendered_parts.push(content.clone());
            }
            sections.push(RenderedSection {
                section,
                content,
                size_bytes,
                entry_ids,
            });
        }

        let rendered_prompt = rendered_parts.join("\n\n");
        let rendered_prompt_hash = digest_hex(rendered_prompt.as_bytes());
        let snapshot = ContextWindowSnapshot {
            window_id: self.window_id.clone(),
            state_hash: before_hash.clone(),
            rendered_prompt_hash,
            total_size_bytes,
            sections,
            rendered_prompt,
        };
        let op_sequence = self.take_sequence();
        let op = self.record_op(
            op_sequence,
            "snapshot",
            reason.into(),
            actor,
            before_hash,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        self.operations.push(op.clone());
        (snapshot, op)
    }

    #[must_use]
    pub fn operations(&self) -> &[ContextOpRecord] {
        &self.operations
    }

    fn has_duplicate(&self, section: &str, content: &str) -> bool {
        let target = digest_hex(content.as_bytes());
        self.entries
            .values()
            .any(|entry| entry.section == section && digest_hex(entry.content.as_bytes()) == target)
    }

    fn section_order(&self) -> Vec<String> {
        let mut order = self.ordered_sections.clone();
        for section in self.entries.values().map(|entry| entry.section.clone()) {
            if !order.iter().any(|existing| existing == &section) {
                order.push(section);
            }
        }
        if self.ordered_sections.is_empty() {
            order.sort();
        }
        order
    }

    fn section_entries(&self, section: &str) -> Vec<&ContextEntry> {
        let mut entries: Vec<&ContextEntry> = self
            .entries
            .values()
            .filter(|entry| entry.section == section)
            .collect();
        entries.sort_by(|a, b| entry_priority_cmp(a, b));
        entries
    }

    #[allow(clippy::too_many_arguments)]
    fn record_op(
        &self,
        sequence: u64,
        op: &str,
        reason: String,
        actor: ContextOperationActor,
        before_hash: String,
        mut added_ids: Vec<String>,
        mut removed_ids: Vec<String>,
        mut truncated_sections: Vec<String>,
        section_reasons: BTreeMap<String, Vec<String>>,
    ) -> ContextOpRecord {
        added_ids.sort();
        removed_ids.sort();
        truncated_sections.sort();
        ContextOpRecord {
            sequence,
            op: op.to_string(),
            reason,
            actor,
            before_hash,
            after_hash: self.state_hash(),
            added_ids,
            removed_ids,
            truncated_sections,
            section_reasons,
        }
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }
}

fn entry_priority_cmp(a: &ContextEntry, b: &ContextEntry) -> Ordering {
    a.depth
        .cmp(&b.depth)
        .then_with(|| a.added_sequence.cmp(&b.added_sequence))
        .then_with(|| a.entry_id.cmp(&b.entry_id))
}

fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn default_policy() -> ContextBudgetPolicy {
        ContextBudgetPolicy {
            total_budget_bytes: 64,
            default_section_budget_bytes: 32,
            per_section_budget_bytes: BTreeMap::new(),
            max_expand_depth: 2,
            deduplicate: true,
            truncation_policy: "ordered_section_then_total_budget".to_string(),
        }
    }

    fn actor() -> ContextOperationActor {
        ContextOperationActor {
            actor: "test".to_string(),
            runtime: None,
            tool: None,
        }
    }

    #[test]
    fn prune_enforces_depth_dedup_and_budgets() {
        let mut window = ContextWindow::new(
            "w1",
            vec!["a".to_string(), "b".to_string()],
            default_policy(),
        );
        let _ = window.add_entry(
            ContextEntryCandidate {
                entry_id: "a0".to_string(),
                section: "a".to_string(),
                content: "header".to_string(),
                source: "s".to_string(),
                depth: 0,
            },
            "seed",
            actor(),
        );
        let _ = window.expand_entries(
            vec![
                ContextEntryCandidate {
                    entry_id: "a1".to_string(),
                    section: "a".to_string(),
                    content: "aaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                    source: "s".to_string(),
                    depth: 1,
                },
                ContextEntryCandidate {
                    entry_id: "a2".to_string(),
                    section: "a".to_string(),
                    content: "bbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                    source: "s".to_string(),
                    depth: 1,
                },
                ContextEntryCandidate {
                    entry_id: "b3".to_string(),
                    section: "b".to_string(),
                    content: "ccccccccccccccccccccccccc".to_string(),
                    source: "s".to_string(),
                    depth: 1,
                },
            ],
            "expand",
            actor(),
            true,
        );
        let prune = window.prune("prune", actor());
        assert!(!prune.removed_ids.is_empty());
        assert!(
            prune
                .section_reasons
                .values()
                .flat_map(|v| v.iter())
                .any(|reason| reason == "section_budget_exceeded"
                    || reason == "total_budget_exceeded")
        );
    }

    #[test]
    fn snapshot_hash_stable_for_same_inputs() {
        let mut w1 = ContextWindow::new("w", vec!["s".to_string()], default_policy());
        let mut w2 = ContextWindow::new("w", vec!["s".to_string()], default_policy());
        for window in [&mut w1, &mut w2] {
            let _ = window.add_entry(
                ContextEntryCandidate {
                    entry_id: "e1".to_string(),
                    section: "s".to_string(),
                    content: "content".to_string(),
                    source: "x".to_string(),
                    depth: 0,
                },
                "seed",
                actor(),
            );
            let _ = window.prune("prune", actor());
        }
        let (s1, _) = w1.snapshot("snapshot", actor());
        let (s2, _) = w2.snapshot("snapshot", actor());
        assert_eq!(s1.state_hash, s2.state_hash);
        assert_eq!(s1.rendered_prompt_hash, s2.rendered_prompt_hash);
    }

    #[test]
    fn snapshot_matches_golden_shape() {
        let mut window = ContextWindow::new(
            "golden",
            vec!["s".to_string(), "t".to_string()],
            default_policy(),
        );
        let _ = window.add_entry(
            ContextEntryCandidate {
                entry_id: "s-1".to_string(),
                section: "s".to_string(),
                content: "S1".to_string(),
                source: "seed".to_string(),
                depth: 0,
            },
            "seed",
            actor(),
        );
        let _ = window.expand_entries(
            vec![
                ContextEntryCandidate {
                    entry_id: "s-2".to_string(),
                    section: "s".to_string(),
                    content: "S2".to_string(),
                    source: "expand".to_string(),
                    depth: 1,
                },
                ContextEntryCandidate {
                    entry_id: "t-1".to_string(),
                    section: "t".to_string(),
                    content: "T1".to_string(),
                    source: "expand".to_string(),
                    depth: 1,
                },
            ],
            "expand",
            actor(),
            true,
        );
        let _ = window.prune("prune", actor());
        let (snapshot, _) = window.snapshot("snapshot", actor());
        insta::assert_json_snapshot!(snapshot, @r###"
        {
          "window_id": "golden",
          "state_hash": "7352e41c13b3b73d73986042820404e9a2a5dad5bcad3edbe59338dc3ff0251b",
          "rendered_prompt_hash": "550e8dab8c77c70ec018005217639e92980d0299b32513e759b6fc9a8329e3c1",
          "total_size_bytes": 8,
          "sections": [
            {
              "section": "s",
              "content": "S1\n\nS2",
              "size_bytes": 6,
              "entry_ids": [
                "s-1",
                "s-2"
              ]
            },
            {
              "section": "t",
              "content": "T1",
              "size_bytes": 2,
              "entry_ids": [
                "t-1"
              ]
            }
          ],
          "rendered_prompt": "S1\n\nS2\n\nT1"
        }
        "###);
    }

    proptest! {
        #[test]
        fn expand_is_order_independent_for_hash(
            ids in prop::collection::vec("[a-z]{1,8}", 1..16)
        ) {
            let mut forward = ContextWindow::new("p", vec!["s".to_string()], default_policy());
            let mut backward = ContextWindow::new("p", vec!["s".to_string()], default_policy());

            let candidates_forward: Vec<ContextEntryCandidate> = ids.iter().map(|id| ContextEntryCandidate {
                entry_id: id.clone(),
                section: "s".to_string(),
                content: format!("content-{id}"),
                source: "seed".to_string(),
                depth: 1,
            }).collect();
            let candidates_backward: Vec<ContextEntryCandidate> = ids.iter().rev().map(|id| ContextEntryCandidate {
                entry_id: id.clone(),
                section: "s".to_string(),
                content: format!("content-{id}"),
                source: "seed".to_string(),
                depth: 1,
            }).collect();

            let _ = forward.expand_entries(candidates_forward, "expand", actor(), true);
            let _ = backward.expand_entries(candidates_backward, "expand", actor(), true);
            let _ = forward.prune("prune", actor());
            let _ = backward.prune("prune", actor());

            prop_assert_eq!(forward.state_hash(), backward.state_hash());
        }
    }
}
