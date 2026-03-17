use super::*;

impl<M: ModelClient> AgentLoop<M> {
    pub(crate) fn soft_token_budget_compaction_limit(&self) -> usize {
        self.config
            .token_budget
            .saturating_mul(Self::SOFT_TOKEN_BUDGET_COMPACTION_PERCENT)
            / 100
    }

    pub(crate) fn should_attempt_preemptive_budget_compaction(
        &self,
        request_tokens: usize,
    ) -> bool {
        self.used_tokens.saturating_add(request_tokens) >= self.soft_token_budget_compaction_limit()
    }

    pub(crate) fn budget_thresholds_crossed(&mut self) -> Vec<u8> {
        if self.config.token_budget == 0 {
            return Vec::new();
        }
        let used_percent = self.used_tokens.saturating_mul(100) / self.config.token_budget;
        let mut crossed = Vec::new();
        for threshold in [70_u8, 85, 95] {
            if used_percent >= usize::from(threshold)
                && !self.emitted_budget_thresholds.contains(&threshold)
            {
                self.emitted_budget_thresholds.push(threshold);
                crossed.push(threshold);
            }
        }
        crossed
    }
}
