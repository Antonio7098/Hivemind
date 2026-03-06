use super::*;

impl ContextWindow {
    #[allow(clippy::too_many_lines)]
    pub fn prune(
        &mut self,
        reason: impl Into<String>,
        actor: ContextOperationActor,
    ) -> ContextOpRecord {
        let before_hash = self.state_hash();
        let mut to_remove: BTreeSet<String> = BTreeSet::new();
        let mut section_reasons: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

        for entry in self.entries.values() {
            if entry.depth > self.budget_policy.max_expand_depth {
                to_remove.insert(entry.entry_id.clone());
                section_reasons
                    .entry(entry.section.clone())
                    .or_default()
                    .insert("max_expand_depth_exceeded".to_string());
            }
        }

        if self.budget_policy.deduplicate {
            for section in self.section_order() {
                let mut seen = HashSet::new();
                for entry in self.section_entries(&section) {
                    if to_remove.contains(&entry.entry_id) {
                        continue;
                    }
                    let key = super::support::digest_hex(entry.content.as_bytes());
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
}
