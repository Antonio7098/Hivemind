use super::*;

impl ContextWindow {
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
}
