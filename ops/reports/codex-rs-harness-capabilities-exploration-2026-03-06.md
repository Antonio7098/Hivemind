# Codex-rs Native Harness Capability Assessment for Hivemind

## Metadata

- Date: 2026-03-06
- Scope: Codex-rs native harness capabilities beyond Phase 4.5 runtime hardening imports
- Primary references:
  - `ops/reports/phase-4.5-codex-rs-runtime-hardening-study-2026-02-28.md`
  - `codex-main/codex-rs/core/src/context_manager/history.rs`
  - `codex-main/codex-rs/core/src/context_manager/normalize.rs`
  - `codex-main/codex-rs/core/src/truncate.rs`
  - `codex-main/codex-rs/core/src/compact.rs`
  - `codex-main/codex-rs/core/src/unified_exec/mod.rs`
  - `codex-main/codex-rs/core/src/unified_exec/process_manager.rs`
  - `codex-main/codex-rs/protocol/src/request_user_input.rs`
  - `codex-main/codex-rs/protocol/src/plan_tool.rs`
  - `codex-main/codex-rs/protocol/src/dynamic_tools.rs`
  - `codex-main/codex-rs/core/src/mcp_connection_manager.rs`
  - `codex-main/codex-rs/core/src/mcp_tool_call.rs`
  - `codex-main/codex-rs/hooks/src/types.rs`
  - `codex-main/codex-rs/hooks/src/registry.rs`

## Executive Summary

The next high-value imports from `codex-rs` are not primarily more hardening controls. They are the runtime-local harness mechanisms that make an agent practical under bounded context windows:

1. A dedicated runtime-local history/context manager.
2. Prompt assembly that distinguishes model-visible context from event-stream data.
3. Session-aware terminal tooling where live transcripts are not blindly replayed into model context.
4. Compaction and truncation policies that keep tool output useful without making the prompt explode.
5. Structured protocol tools such as `request_user_input`, `update_plan`, and dynamic tool registration.
6. Optional ecosystem layers: MCP, hooks/notifications, and later collaborative subagents.

Hivemind already has the stronger system-level foundation:

- event sourcing and replay,
- retry context,
- deterministic context-window operations,
- governance and attribution,
- runtime hardening.

Codex-rs is stronger in the runtime-local working-memory layer:

- turn-item history,
- prompt-safe tool result retention,
- session-aware exec semantics,
- context compaction,
- native interaction protocol tools.

The best synthesis is:

- Hivemind owns truth.
- The native harness owns working memory.

That preserves the Hivemind rule that the model context window is operational state, not authoritative state.

## Key Finding About Hivemind's Current Native Harness

The current Hivemind native harness appears to still be closer to a directive loop than a fully interactive tool loop.

Observed baseline:

- `src/native/mod.rs`
- `src/native/adapter.rs`
- `src/native/tool_engine.rs`
- `src/core/context_window.rs`

The most important gap is architectural:

1. `AgentLoop::run(...)` operates on a prompt plus an optional opaque context string.
2. Tool execution is not the center of an in-loop conversation history in the Codex sense.
3. Model-visible history does not yet appear to be a first-class sequence of user/tool/result/summary items.

This means the highest-leverage next step is to make the native harness a real tool-result loop before importing higher-level capability surfaces.

## Capability Area 1: Runtime-Local History and Prompt Assembly

Relevant `codex-rs` surfaces:

- `core/src/context_manager/history.rs`
- `core/src/context_manager/normalize.rs`
- `core/src/state/session.rs`
- `core/src/codex.rs`

What Codex does well:

- Maintains an ordered history of turn items.
- Separates storage/persistence concerns from prompt-visible history assembly.
- Normalizes malformed or incomplete history before sending it back to the model.
- Builds prompt input from a curated subset of prior items rather than replaying raw logs.

What Hivemind should adopt:

- A runtime-local `TurnItem` family for native execution.
- A prompt assembler that explicitly includes:
  - base system instructions,
  - task and retry context,
  - selected history items,
  - tool contracts,
  - compacted summaries,
  - provenance hashes.
- A distinction between:
  - event-visible artifacts,
  - runtime working memory,
  - model-visible prompt items.

Why this matters:

Without this layer, `request_user_input`, plan updates, dynamic tools, and long-lived terminal sessions all become awkward because there is no natural place to keep bounded, structured working memory.

## Capability Area 2: Context Budgeting, Truncation, and Compaction

Relevant `codex-rs` surfaces:

- `core/src/truncate.rs`
- `core/src/compact.rs`
- `core/src/rollout/policy.rs`

What Codex does well:

- Estimates size/tokens for prompt assembly.
- Truncates outputs at record time instead of waiting until the prompt is already too large.
- Applies persistence policies to history items.
- Compacts older history into summary forms when context pressure grows.

What Hivemind should adopt:

- A runtime-local budget manager separate from the deterministic orchestration context window.
- Per-item truncation rules for tool output, especially terminal output.
- Summary items and compaction markers with source references and hashes.
- Prompt headroom rules so the model always has space for the current turn.

Important Hivemind constraint:

This must remain reconstructable and subordinate to events. A compacted summary can be prompt-visible, but the source truth should still be event/journal backed.

## Capability Area 3: Session-Aware Terminal Tooling

Relevant `codex-rs` surfaces:

- `core/src/unified_exec/mod.rs`
- `core/src/unified_exec/process_manager.rs`
- `core/src/unified_exec/process.rs`

What Codex does well:

- Treats `exec_command` and `write_stdin` as persistent session operations.
- Tracks process IDs, lifecycle, output limits, and session cleanup.
- Streams output live for observers while also returning bounded model-facing output.
- Avoids making raw PTY history synonymous with model context.

Recommendation for Hivemind terminal semantics:

Use a three-layer model.

1. Live terminal transcript
   - raw stdout/stderr chunks,
   - stdin writes,
   - process lifecycle,
   - warnings and exits,
   - consumed by UI and event observers.

2. Session working memory
   - bounded rolling transcript,
   - command metadata,
   - cwd,
   - session state,
   - waiting-for-input hints,
   - last useful output,
   - compaction checkpoints.

3. Model-visible context item
   - command,
   - short bounded output,
   - exit state or still-running state,
   - truncation markers,
   - optional session summary.

Key rule:

Do not put full PTY transcripts into the context window. Stream them to events and UI, keep a bounded session transcript in runtime state, and inject only bounded slices or summaries into model history.

## Capability Area 4: Structured Protocol Tools

Relevant `codex-rs` surfaces:

- `protocol/src/request_user_input.rs`
- `protocol/src/plan_tool.rs`
- `protocol/src/dynamic_tools.rs`

These are good fits for Hivemind, but they become much more valuable after the native harness has real history and tool-loop semantics.

### 4.1 `request_user_input`

Useful for:

- multi-question clarification flows,
- secure/sensitive answer fields,
- operator-driven branching,
- attributable answer capture.

Hivemind fit:

- capture answers as explicit events,
- link answers to attempt/turn provenance,
- feed answers back into runtime-local history as user-input items.

### 4.2 `update_plan`

Useful for:

- operator-visible plan state,
- structured progress updates,
- attempt-level plan continuity.

Hivemind fit:

- model can propose plan updates,
- Hivemind persists the plan timeline as projections/events,
- enforce deterministic mutation rules such as a single in-progress guard.

### 4.3 Dynamic tools

Useful for:

- runtime-registered tools,
- per-attempt/per-thread capabilities,
- schema-validated tool federation.

Hivemind fit:

- dynamic tool definitions should be replay-safe,
- the registration lifecycle should be evented,
- the native loop should treat them exactly like built-ins once registered.

## Capability Area 5: MCP Integration and External Tool Federation

Relevant `codex-rs` surfaces:

- `core/src/mcp_connection_manager.rs`
- `core/src/mcp_tool_call.rs`
- protocol and server surfaces under the MCP modules.

What Codex adds:

- tool federation through external MCP servers,
- approval-aware external tool calls,
- a standard connector surface instead of endlessly growing built-ins.

Hivemind recommendation:

- defer this until the native harness has stable local history and session semantics,
- then add MCP as a federation layer on top of the same tool-call contracts,
- preserve approval, sandbox, and provenance tagging across MCP boundaries.

## Capability Area 6: Hooks and Notifications

Relevant `codex-rs` surfaces:

- `hooks/src/types.rs`
- `hooks/src/registry.rs`

What Codex adds:

- after-agent and after-tool callbacks,
- external process triggers,
- configurable notification surfaces.

Hivemind recommendation:

- add this after the core harness loop is stable,
- keep hooks explicitly best-effort or policy-guarded,
- do not let them become hidden control flow.

Hooks are excellent for observability and operator automation, but should remain subordinate to the event spine.

## Capability Area 7: Runtime Turn Journaling and Resume

Codex persists session and rollout information in a way that can be resumed. Hivemind already has stronger event authority, so the correct import is not Codex's exact persistence model.

Recommended Hivemind variant:

- add a runtime turn journal artifact per invocation,
- capture prompt delivery hashes,
- record tool call and tool result references,
- checkpoint session summaries,
- store compaction markers and source references,
- reconstruct runtime-local working memory from journal + events on resume.

This gives the native harness practical resumability without making prompt memory authoritative.

## Capability Area 8: Lower-Priority Collaborative Subagents

Relevant `codex-rs` surface:

- `core/src/tools/handlers/multi_agents.rs`

This is interesting but not foundational for Hivemind because Hivemind already is an orchestration system.

Recommendation:

- treat collaborative subagents as a later optimization,
- only add them after the single-agent native harness has strong context and session semantics,
- keep them bounded and attributable if introduced.

## Recommended Adoption Order

### Priority 0: Harness foundations

1. Refactor the native harness into a real tool-result loop.
2. Add runtime-local history and prompt assembly.
3. Split model-visible context from raw event-stream output.

### Priority 1: Terminal and context behavior

1. Make exec sessions context-aware instead of transcript-dumping.
2. Add truncation, compaction, and session summaries.
3. Add runtime turn journals and resume checkpoints.

### Priority 2: Capability lift

1. `request_user_input`
2. `update_plan`
3. dynamic tools

### Priority 3: Ecosystem

1. MCP integration
2. hooks/notifications
3. optional collaborative subagents

## Recommended Design Principle for Hivemind

Preserve this split:

- Hivemind owns truth:
  - events,
  - projections,
  - manifests,
  - attempt lineage,
  - retry context,
  - governance.

- Native harness owns working memory:
  - prompt assembly,
  - bounded history,
  - tool outputs,
  - terminal session summaries,
  - compaction,
  - resume scaffolding.

This is the cleanest path to importing Codex's practical harness behavior without compromising Hivemind's determinism model.

## Recommendation

Proceed with a dedicated follow-on phase that focuses on native harness context, session semantics, and capability lift after Phase 4.5 hardening. The attached roadmap should be tracked as:

- `ops/roadmap/phase-4.7-native-harness-context-and-capability-lift.md`

That phase should intentionally absorb the Phase 4.5 Sprint 58 capability-lift work, but only after the harness foundations needed to make those capabilities reliable are in place.

