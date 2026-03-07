use super::*;
use sha2::{Digest, Sha256};

impl ContextWindow {
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

    #[must_use]
    pub fn operations(&self) -> &[ContextOpRecord] {
        &self.operations
    }

    pub(super) fn has_duplicate(&self, section: &str, content: &str) -> bool {
        let target = digest_hex(content.as_bytes());
        self.entries
            .values()
            .any(|entry| entry.section == section && digest_hex(entry.content.as_bytes()) == target)
    }

    pub(super) fn section_order(&self) -> Vec<String> {
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

    pub(super) fn section_entries(&self, section: &str) -> Vec<&ContextEntry> {
        let mut entries: Vec<&ContextEntry> = self
            .entries
            .values()
            .filter(|entry| entry.section == section)
            .collect();
        entries.sort_by(|a, b| entry_priority_cmp(a, b));
        entries
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_op(
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

    pub(super) fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }
}

pub(super) fn entry_priority_cmp(a: &ContextEntry, b: &ContextEntry) -> Ordering {
    a.depth
        .cmp(&b.depth)
        .then_with(|| a.added_sequence.cmp(&b.added_sequence))
        .then_with(|| a.entry_id.cmp(&b.entry_id))
}

pub(super) fn digest_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
