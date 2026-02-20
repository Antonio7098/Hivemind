---
title: Retry Context
description: Retry semantics and context
order: 5
---

# Hivemind — Retry Context

> **Principle 15:** No magic. Everything has a reason. Everything has a trail.
> **Principle 1:** Observability is truth.

This document specifies how **retry context** is assembled and delivered to worker agents. When a task retries, the new attempt must receive explicit, structured information about what happened before.

Sprint 39 extends this model with an immutable **attempt context manifest** that freezes governance inputs for every attempt. Retry context remains explicit, but now lives alongside a deterministic manifest/hash trail.

There is no implicit memory. There is no hidden state. Retry context is the mechanism by which agents learn from prior attempts.

---

## 1. The Retry Problem

### 1.1 Why Retry Context Matters

When a worker agent retries a task:
- The agent has no memory of prior attempts (by design)
- The agent cannot see what it did before
- The agent cannot see why it failed

Without explicit context, retries are blind repetition.

### 1.2 The Solution

Hivemind assembles **retry context**: structured information about prior attempts delivered as explicit input to the new attempt.

Retry context is:
- Mechanically derived (from events and artifacts)
- Explicitly passed (not injected into hidden memory)
- Observable (recorded as part of attempt input)

---

## 2. Retry Context Structure

### 2.1 Complete Structure

```
RetryContext:
  # Metadata
  current_attempt: int
  max_attempts: int
  task_id: string

  # Prior Attempts
  prior_attempts: [AttemptSummary]

  # Aggregate Feedback
  primary_failure_reason: string
  actionable_feedback: string

  # Artifacts
  last_diff: string | null
  cumulative_changes: string | null
```

### 2.2 AttemptSummary Structure

```
AttemptSummary:
  attempt_number: int
  duration_ms: int

  # Outcome
  outcome: SCOPE_VIOLATION | CHECK_FAILURE | VERIFIER_REJECTION | CRASH | TIMEOUT

  # Check Results
  check_results: [CheckResultSummary]

  # Verifier Feedback
  verifier_outcome: PASS | SOFT_FAIL | HARD_FAIL | null
  verifier_feedback: string | null

  # Changes Made
  files_modified: [string]
  diff_summary: string
```

### 2.3 CheckResultSummary Structure

```
CheckResultSummary:
  name: string
  passed: boolean
  output_excerpt: string  # First N lines of output
```

---

## 3. Context Assembly

### 3.1 Assembly Process

When transitioning to RETRY state:

1. **Gather Prior Attempts**
   - Query events for all AttemptCompleted events for this task
   - Extract outcome, timing, and artifacts from each

2. **Summarize Check Results**
   - For each prior attempt, extract check results
   - Truncate verbose output to relevant excerpts

3. **Extract Verifier Feedback**
   - Get verifier decision and feedback from most recent attempt
   - Include feedback from earlier attempts if relevant

4. **Compute Diff**
   - Extract diff from most recent attempt
   - Optionally compute cumulative diff across attempts

5. **Synthesize Actionable Feedback**
   - Combine check failures and verifier feedback
   - Prioritize most actionable information

### 3.2 Assembly Rules

**From Events (Mechanically Derived):**
- Attempt timing
- Outcome classification
- Check pass/fail status
- File modification list

**From Artifacts:**
- Actual diffs
- Check output (truncated)

**From Verifier (LLM-Generated, Advisory):**
- Verifier feedback text
- Verifier reasoning

### 3.3 Context Size Limits

Retry context has size limits to avoid overwhelming the worker:

```
max_prior_attempts_included: 3
max_diff_lines: 500
max_check_output_lines: 50
max_verifier_feedback_chars: 2000
```

If limits are exceeded, prioritize:
1. Most recent attempt
2. Most actionable feedback
3. Failing check output over passing

---

## 4. Context Delivery

### 4.1 Delivery Mechanism

Retry context is delivered as **explicit input** to the worker agent.

It is NOT:
- Injected into model context invisibly
- Stored in runtime memory
- Passed through environment variables

It IS:
- Part of the task prompt
- Visible in attempt input event
- Observable and auditable

### 4.2 Prompt Structure

```
# Task: {task.title}

## Description
{task.description}

## Success Criteria
{task.success_criteria}

## Retry Context

This is attempt {retry_context.current_attempt} of {retry_context.max_attempts}.

### Prior Attempt Summary
{formatted_prior_attempts}

### What Went Wrong
{retry_context.primary_failure_reason}

### Actionable Feedback
{retry_context.actionable_feedback}

### Previous Changes (for reference)
```diff
{retry_context.last_diff}
```

## Instructions
Based on the above context, please complete the task. Address the issues identified in prior attempts.
```

### 4.3 Formatting Rules

- Prior attempts are summarized, not dumped raw
- Diffs are syntax-highlighted where possible
- Check output is truncated with `[... N lines omitted]`
- Verifier feedback is quoted verbatim

---

## 5. Feedback Sources

### 5.1 Mechanical Feedback (Authoritative)

Mechanical feedback is derived from facts, not LLM judgment.

**Check Failures:**
```
Check 'pytest' failed with exit code 1.
Output:
  FAILED tests/test_auth.py::test_login - AssertionError: expected 200, got 401
  1 failed, 42 passed
```

**Scope Violations:**
```
Scope violation detected.
Modified file outside allowed scope: /etc/passwd
Allowed write paths: src/**, tests/**
```

**Timeouts:**
```
Attempt timed out after 300 seconds.
Last observed activity: writing to src/main.py
```

### 5.2 Advisory Feedback (From Verifier)

Verifier feedback is LLM-generated and may be wrong.

**Presentation:**
```
### Verifier Feedback (Advisory)
The verifier agent provided the following feedback:

> The implementation handles the happy path but does not handle the case
> where the user is not authenticated. Consider adding a check for
> authentication status before proceeding.

Note: This feedback is advisory and may not be accurate.
```

### 5.3 Feedback Priority

When assembling `actionable_feedback`, prioritize:

1. **Deterministic failures** (check output, scope violations)
2. **Specific errors** (stack traces, assertion failures)
3. **Verifier feedback** (advisory, may be wrong)
4. **Generic observations** (timing, file changes)

---

## 6. Cumulative Learning

### 6.1 Attempt History

Each attempt adds to the history:

```
Attempt 1: Modified src/auth.py, tests failed
Attempt 2: Modified src/auth.py differently, tests still failed
Attempt 3: (current)
```

This history helps the worker agent avoid repeating mistakes.

### 6.2 Pattern Detection

Retry context may include detected patterns:

```
### Observed Patterns
- Attempts 1 and 2 both failed the same test (test_login)
- Attempts 1 and 2 both modified the same file (src/auth.py)
- Consider a different approach
```

Pattern detection is mechanical, not LLM-based.

### 6.3 Drift Warning

If attempts are diverging rather than converging:

```
### Warning
Prior attempts have made significantly different changes.
This may indicate the task is under-specified or the approach is wrong.
Consider requesting human review.
```

---

## 7. Context Events

### 7.1 Context Assembly Event

```
RetryContextAssembled:
  task_id: string
  attempt_number: int
  prior_attempts_count: int
  context_size_bytes: int
  feedback_sources: [string]  # e.g., ["check:pytest", "verifier"]

AttemptContextAssembled:
  flow_id: string
  task_id: string
  attempt_id: string
  manifest_hash: string
  inputs_hash: string
  context_hash: string  # rendered prompt hash from snapshot
  manifest_json: string

ContextWindowCreated:
  flow_id: string
  task_id: string
  attempt_id: string
  window_id: string
  policy: ordered_section_then_total_budget
  state_hash: string

ContextOpApplied:
  flow_id: string
  task_id: string
  attempt_id: string
  window_id: string
  op: add|remove|expand|prune|snapshot
  reason: string
  actor: string
  runtime: string|null
  tool: string|null
  before_hash: string
  after_hash: string
  section_reasons: {string: [string]}

ContextWindowSnapshotCreated:
  flow_id: string
  task_id: string
  attempt_id: string
  window_id: string
  state_hash: string
  rendered_prompt_hash: string
  delivered_input_hash: string
```

### 7.2 Context Delivery Event

```
AttemptContextDelivered:
  flow_id: string
  task_id: string
  attempt_id: string
  manifest_hash: string
  inputs_hash: string
  context_hash: string  # delivered runtime context hash
  prior_manifest_hashes: [string]
```

### 7.3 Override and Truncation Telemetry

```
AttemptContextOverridesApplied:
  attempt_id: string
  template_document_ids: [string]
  included_document_ids: [string]
  excluded_document_ids: [string]
  resolved_document_ids: [string]

AttemptContextTruncated:
  attempt_id: string
  budget_bytes: int
  original_size_bytes: int
  truncated_size_bytes: int
  sections: [string]
  section_reasons: {string: [string]}
  policy: ordered_section_then_total_budget
```

---

## 8. Observability

### 8.1 Context Inspection

Retry context is fully inspectable:
- Stored as part of attempt record
- Viewable via CLI: `hivemind attempt inspect <attempt_id> --context`
- Included in attempt events
- Bundled with immutable manifest v2 + hash metadata (`manifest_hash`, `inputs_hash`, `context_window_hash`, `rendered_prompt_hash`, `delivered_context_hash`)

### 8.2 Debugging Retries

When debugging why retries aren't working:

1. **Check context assembly:** Was the right information included?
2. **Check context delivery:** Was it formatted correctly?
3. **Check feedback quality:** Was the feedback actionable?

### 8.3 Context Quality Metrics

Track over time:
- Average context size
- Feedback source distribution
- Retry success rate by feedback type

---

## 9. Limitations

### 9.1 What Context Cannot Fix

Retry context cannot help if:
- The task is impossible
- The success criteria are contradictory
- The scope is too narrow for the task
- The approach is fundamentally wrong

### 9.2 Verifier Feedback Quality

Verifier feedback is LLM-generated:
- May be wrong
- May be misleading
- May fixate on irrelevant issues

Mechanical feedback should always be prioritized.

### 9.3 Context Window Limits

If retry context exceeds worker model's context window:
- Truncate oldest attempt details first
- Preserve most recent attempt fully
- Preserve all check failure output

---

## 10. Configuration

### 10.1 Task-Level Config

```yaml
task:
  retry:
    max_attempts: 3
    context:
      include_prior_diffs: true
      include_check_output: true
      include_verifier_feedback: true
      max_context_tokens: 4000
```

### 10.2 Project-Level Defaults

```yaml
project:
  retry_defaults:
    max_attempts: 2
    context:
      include_prior_diffs: true
      include_check_output: true
      include_verifier_feedback: true
```

---

## 11. Invariants

The retry context model guarantees:

- Retry context is explicitly assembled from events and artifacts
- Retry context is delivered as visible input, not hidden state
- Retry context is observable and auditable
- Retry lineage is explicit via prior attempt manifest hashes
- Mechanical feedback is always included
- Advisory feedback is clearly labeled as advisory

Violating these invariants is a SystemError.

---

## 12. Summary

Retry context is how Hivemind enables agents to learn from failure without implicit memory:

- **Explicit:** Context is assembled and delivered, not remembered
- **Observable:** Context is recorded and inspectable
- **Structured:** Context separates facts from advice
- **Honest:** Advisory feedback is labeled as such

> The agent does not remember. The system remembers for it.

This is how Hivemind maintains Principle 15 (no magic) while enabling effective retries.
