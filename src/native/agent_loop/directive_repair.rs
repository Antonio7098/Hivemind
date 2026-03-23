use super::*;

impl<M: ModelClient> AgentLoop<M> {
    pub(crate) fn parse_directive(raw: &str) -> Result<ModelDirective, NativeRuntimeError> {
        let raw = raw.trim();
        if let Some(directive) = ModelDirective::parse_relaxed(raw) {
            return Ok(directive);
        }
        Err(NativeRuntimeError::MalformedModelOutput {
            raw_output: raw.to_string(),
            expected: "THINK:<message> | ACT:<action> | DONE:<summary>".to_string(),
            recovery_hint: "Return one explicit directive with a known prefix (THINK/ACT/DONE)"
                .to_string(),
        })
    }

    pub(crate) fn malformed_output_repair_item(
        invocation_id: &str,
        turn_index: u32,
        repair_attempt: u8,
        raw_output: &str,
    ) -> TurnItem {
        let raw_output = if raw_output.chars().count() > 600 {
            let mut truncated = raw_output.chars().take(600).collect::<String>();
            truncated.push_str(" …");
            truncated
        } else {
            raw_output.to_string()
        };
        user_input_item(invocation_id, turn_index.saturating_mul(100).saturating_add(90).saturating_add(u32::from(repair_attempt)), "controller_repair", format!("Your previous response could not be parsed as one native directive. Return exactly one line beginning with THINK:, ACT:, or DONE:. Do not include prose before the directive. If acting, use ACT:tool:<name>:<json_object>. Previous response:\n{raw_output}"), "runtime.repair")
    }
}
