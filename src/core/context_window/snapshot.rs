use super::support::digest_hex;
use super::*;

impl ContextWindow {
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
}
