use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub task_description: String,
    pub success_criteria: String,
    pub context: Option<String>,
    pub prior_attempts: Vec<AttemptSummary>,
    pub verifier_feedback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptSummary {
    pub attempt_number: u32,
    pub summary: String,
    pub failure_reason: Option<String>,
}

#[must_use]
pub fn format_execution_prompt(input: &ExecutionInput) -> String {
    let mut prompt = format!("Task: {}\n\n", input.task_description);
    let _ = write!(prompt, "Success Criteria: {}\n\n", input.success_criteria);

    if let Some(ref context) = input.context {
        let _ = write!(prompt, "Context:\n{context}\n\n");
    }
    if !input.prior_attempts.is_empty() {
        prompt.push_str("Prior Attempts:\n");
        for attempt in &input.prior_attempts {
            let _ = writeln!(
                prompt,
                "- Attempt {}: {}",
                attempt.attempt_number, attempt.summary
            );
            if let Some(ref reason) = attempt.failure_reason {
                let _ = writeln!(prompt, "  Failure: {reason}");
            }
        }
        prompt.push('\n');
    }
    if let Some(ref feedback) = input.verifier_feedback {
        let _ = write!(prompt, "Verifier Feedback:\n{feedback}\n\n");
    }

    prompt
}
