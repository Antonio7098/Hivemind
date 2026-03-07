# Codex-rs Harness Implementation Guide for Hivemind

**Date:** 2026-03-06  
**Purpose:** Concrete code patterns and file locations for adopting codex-rs capabilities

---

## Quick Reference: File Locations

### Protocol Definitions (codex-rs)
- `protocol/src/request_user_input.rs` – User input contract (45 lines)
- `protocol/src/plan_tool.rs` – Plan management (30 lines)
- `protocol/src/dynamic_tools.rs` – Dynamic tool lifecycle (40 lines)
- `protocol/src/mcp.rs` – MCP types (329 lines)
- `protocol/src/config_types.rs` – Collaboration modes, reasoning effort

### Core Handlers (codex-rs)
- `core/src/tools/handlers/request_user_input.rs` – User input handler
- `core/src/tools/handlers/plan.rs` – Plan update handler
- `core/src/tools/handlers/dynamic_tools.rs` – Dynamic tool handler
- `core/src/tools/sandboxing.rs` – Approval requirements (lines 118-183)
- `core/src/exec_policy.rs` – Policy evaluation (lines 160-184)

### Hooks & Observability (codex-rs)
- `hooks/src/types.rs` – Hook event types (287 lines)
- `hooks/src/registry.rs` – Hook dispatch (483 lines)
- `otel/src/lib.rs` – Metrics and telemetry (lines 108-185)

### Session & State (codex-rs)
- `core/src/state/session.rs` – Session state struct (lines 14-53)
- `state/src/runtime.rs` – SQLite runtime (lines 27-50)
- `core/src/state_db.rs` – State DB integration (lines 1-95)

### MCP Server (codex-rs)
- `mcp-server/src/lib.rs` – MCP server main (155 lines)
- `mcp-server/src/message_processor.rs` – Message routing
- `docs/codex_mcp_interface.md` – MCP API documentation

---

## Implementation Patterns

### Pattern 1: Tool Handler Structure
```rust
// From codex-rs/core/src/tools/handlers/plan.rs
async fn handle_update_plan(
    session: &Session,
    turn_context: &TurnContext,
    arguments: String,
    call_id: String,
) -> Result<String, FunctionCallError> {
    // 1. Validate mode
    if turn_context.collaboration_mode.mode == ModeKind::Plan {
        return Err(FunctionCallError::RespondToModel(
            "update_plan is not allowed in Plan mode".to_string(),
        ));
    }
    
    // 2. Parse arguments
    let args = parse_update_plan_arguments(&arguments)?;
    
    // 3. Emit event
    session.send_event(turn_context, EventMsg::PlanUpdate(args)).await;
    
    // 4. Return to model
    Ok("Plan updated".to_string())
}
```

### Pattern 2: Hook Dispatch
```rust
// From codex-rs/hooks/src/registry.rs
pub async fn dispatch(&self, hook_payload: HookPayload) -> Vec<HookResponse> {
    let hooks = self.hooks_for_event(&hook_payload.hook_event);
    let mut outcomes = Vec::with_capacity(hooks.len());
    
    for hook in hooks {
        let outcome = hook.execute(&hook_payload).await;
        let should_abort = outcome.result.should_abort_operation();
        outcomes.push(outcome);
        
        if should_abort {
            break;  // Stop on FailedAbort
        }
    }
    outcomes
}
```

### Pattern 3: Approval Requirement Evaluation
```rust
// From codex-rs/core/src/tools/sandboxing.rs
pub fn default_exec_approval_requirement(
    policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
) -> ExecApprovalRequirement {
    let needs_approval = match policy {
        AskForApproval::Never | AskForApproval::OnFailure => false,
        AskForApproval::OnRequest => !matches!(
            sandbox_policy,
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        ),
        AskForApproval::UnlessTrusted => true,
    };
    
    if needs_approval {
        ExecApprovalRequirement::NeedsApproval {
            reason: None,
            proposed_execpolicy_amendment: None,
        }
    } else {
        ExecApprovalRequirement::Skip {
            bypass_sandbox: false,
            proposed_execpolicy_amendment: None,
        }
    }
}
```

### Pattern 4: Hook Event Serialization
```rust
// From codex-rs/hooks/src/types.rs
#[derive(Debug, Serialize, Clone)]
pub struct HookPayload {
    pub session_id: ThreadId,
    pub cwd: PathBuf,
    #[serde(serialize_with = "serialize_triggered_at")]
    pub triggered_at: DateTime<Utc>,
    pub hook_event: HookEvent,
}

// Serializes to stable JSON for external processes
// Example: {"session_id":"...", "cwd":"/tmp", "triggered_at":"2025-01-01T00:00:00Z", ...}
```

### Pattern 5: Dynamic Tool Response Handling
```rust
// From codex-rs/app-server/src/dynamic_tools.rs
pub async fn on_call_response(
    call_id: String,
    receiver: oneshot::Receiver<ClientRequestResult>,
    conversation: Arc<CodexThread>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(Ok(value)) => value,
        Ok(Err(err)) => {
            // Handle client error
            let fallback = CoreDynamicToolResponse {
                content_items: vec![...],
                success: false,
            };
            conversation.submit(Op::DynamicToolResponse {
                id: call_id,
                response: fallback,
            }).await;
            return;
        }
        Err(err) => { /* Handle channel error */ }
    };
    
    // Deserialize and submit response
    let response = serde_json::from_value::<DynamicToolCallResponse>(value)?;
    conversation.submit(Op::DynamicToolResponse {
        id: call_id,
        response: response.into(),
    }).await;
}
```

---

## Key Data Structures

### RequestUserInputQuestion
```rust
pub struct RequestUserInputQuestion {
    pub id: String,
    pub header: String,
    pub question: String,
    pub is_other: bool,
    pub is_secret: bool,
    pub options: Option<Vec<RequestUserInputQuestionOption>>,
}
```

### UpdatePlanArgs
```rust
pub struct UpdatePlanArgs {
    pub explanation: Option<String>,
    pub plan: Vec<PlanItemArg>,
}

pub struct PlanItemArg {
    pub step: String,
    pub status: StepStatus,  // Pending, InProgress, Completed
}
```

### HookEventAfterToolUse
```rust
pub struct HookEventAfterToolUse {
    pub turn_id: String,
    pub call_id: String,
    pub tool_name: String,
    pub tool_kind: HookToolKind,  // Function, Custom, LocalShell, Mcp
    pub tool_input: HookToolInput,
    pub executed: bool,
    pub success: bool,
    pub duration_ms: u64,
    pub mutating: bool,
    pub sandbox: String,
    pub sandbox_policy: String,
    pub output_preview: String,
}
```

### ExecApprovalRequirement
```rust
pub enum ExecApprovalRequirement {
    Skip {
        bypass_sandbox: bool,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    NeedsApproval {
        reason: Option<String>,
        proposed_execpolicy_amendment: Option<ExecPolicyAmendment>,
    },
    Forbidden {
        reason: String,
    },
}
```

---

## Integration Checklist

- [ ] Port protocol types to Hivemind
- [ ] Implement tool handlers
- [ ] Add hook system with registry
- [ ] Implement approval routing and caching
- [ ] Add SQLite state persistence
- [ ] Instrument metrics collection
- [ ] Create MCP server binary
- [ ] Add RPC endpoints for new capabilities
- [ ] Wire message processor routing
- [ ] Add config schema for new features
- [ ] Write integration tests
- [ ] Document API changes

---

## Testing Patterns (from codex-rs)

**Hook dispatch test:**
```rust
#[tokio::test]
async fn dispatch_executes_multiple_hooks() {
    let hooks = Hooks {
        after_agent: vec![hook1, hook2],
        ..Hooks::default()
    };
    let outcomes = hooks.dispatch(payload).await;
    assert_eq!(outcomes.len(), 2);
}
```

**Approval requirement test:**
```rust
#[tokio::test]
async fn exec_approval_respects_policy() {
    let requirement = manager
        .create_exec_approval_requirement_for_command(ExecApprovalRequest {
            command: &["rm".to_string()],
            approval_policy: AskForApproval::UnlessTrusted,
            sandbox_policy: &SandboxPolicy::ReadOnly { ... },
            ...
        })
        .await;
    assert!(matches!(requirement, ExecApprovalRequirement::NeedsApproval { ... }));
}
```

---

## References

- Codex-rs source: `/home/antonio/programming/Hivemind/codex-main/codex-rs/`
- Phase 4.5 study: `hivemind/ops/reports/phase-4.5-codex-rs-runtime-hardening-study-2026-02-28.md`
- Protocol docs: `codex-main/codex-rs/docs/protocol_v1.md`

