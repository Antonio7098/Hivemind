# Codex-rs Harness Adoption Roadmap for Hivemind

**Date:** 2026-03-06  
**Objective:** Phased adoption of 9 higher-level agent harness capabilities

---

## Capability Dependency Graph

```
┌─────────────────────────────────────────────────────────────┐
│ Foundation Layer (Weeks 1-2)                                │
├─────────────────────────────────────────────────────────────┤
│ • Approval Orchestration (3-4d)                             │
│   - ExecApprovalRequirement enum                            │
│   - Policy evaluation logic                                 │
│   - Caching and amendments                                  │
│                                                             │
│ • Hooks System (3-4d)                                       │
│   - HookEvent types (AfterAgent, AfterToolUse)             │
│   - Hook registry and dispatch                              │
│   - External process spawning                               │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Observability Layer (Weeks 3-4)                             │
├─────────────────────────────────────────────────────────────┤
│ • Session Persistence (4-5d)                                │
│   - SQLite schema and migrations                            │
│   - SessionState struct                                     │
│   - CRUD operations                                         │
│                                                             │
│ • Runtime Metrics (2-3d)                                    │
│   - OTEL manager integration                                │
│   - Counter, histogram, duration tracking                   │
│   - Tag-based context                                       │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Integration Layer (Weeks 5-6)                               │
├─────────────────────────────────────────────────────────────┤
│ • MCP Integration (4-5d)                                    │
│   - JSON-RPC message loop                                   │
│   - MCP protocol types                                      │
│   - Message processor routing                               │
│   - Approval request/response flow                          │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ User Interaction Layer (Weeks 7-8)                          │
├─────────────────────────────────────────────────────────────┤
│ • Structured User Input (2-3d)                              │
│   - RequestUserInputQuestion contract                       │
│   - Handler implementation                                  │
│   - RPC endpoint                                            │
│                                                             │
│ • Plan Management (1-2d)                                    │
│   - UpdatePlanArgs contract                                 │
│   - PlanUpdate event emission                               │
│   - Plan state tracking                                     │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Ecosystem Layer (Weeks 9-10)                                │
├─────────────────────────────────────────────────────────────┤
│ • Dynamic Tools (3-4d)                                      │
│   - DynamicToolCallRequest contract                         │
│   - Oneshot channel plumbing                                │
│   - Schema validation                                       │
│                                                             │
│ • Advanced Protocol (2-3d)                                  │
│   - Collaboration modes                                     │
│   - Reasoning effort                                        │
│   - Output schema constraints                               │
│   - Personality override                                    │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Foundation (Weeks 1-2, ~25 days)

### Approval Orchestration (3-4 days)
**Files to port:**
- `core/src/tools/sandboxing.rs` (ExecApprovalRequirement)
- `core/src/exec_policy.rs` (policy evaluation)

**Deliverables:**
- [ ] `ExecApprovalRequirement` enum in protocol
- [ ] `ReviewDecision` enum in protocol
- [ ] Approval request/response routing
- [ ] Decision caching by command hash
- [ ] Amendment storage and application

**Tests:**
- Policy evaluation with different approval modes
- Caching behavior
- Amendment application

### Hooks System (3-4 days)
**Files to port:**
- `hooks/src/types.rs` (HookEvent, HookPayload)
- `hooks/src/registry.rs` (Hooks, dispatch)

**Deliverables:**
- [ ] `HookEvent` enum (AfterAgent, AfterToolUse)
- [ ] `HookPayload` struct with serialization
- [ ] `Hooks` registry with dispatch logic
- [ ] External process spawning
- [ ] Config schema for hook definitions

**Tests:**
- Hook dispatch with multiple hooks
- FailedAbort stops subsequent hooks
- External process execution
- Payload serialization stability

---

## Phase 2: Observability (Weeks 3-4, ~20 days)

### Session Persistence (4-5 days)
**Files to port:**
- `state/src/runtime.rs` (StateRuntime)
- `core/src/state/session.rs` (SessionState)

**Deliverables:**
- [ ] SQLite schema (sessions, history, metadata tables)
- [ ] StateRuntime with pool management
- [ ] SessionState struct
- [ ] CRUD operations
- [ ] Migration support
- [ ] Feature flag `sqlite`

**Tests:**
- Session creation and retrieval
- History persistence
- Metadata storage
- Migration execution

### Runtime Metrics (2-3 days)
**Files to port:**
- `otel/src/lib.rs` (OtelManager)

**Deliverables:**
- [ ] Metric names (turn.duration, tool.execution, etc.)
- [ ] Counter, histogram, duration tracking
- [ ] Tag-based context
- [ ] Snapshot and summary methods
- [ ] OTEL exporter integration

**Tests:**
- Counter increments
- Histogram recording
- Duration tracking
- Tag validation

---

## Phase 3: Integration (Weeks 5-6, ~20 days)

### MCP Integration (4-5 days)
**Files to port:**
- `protocol/src/mcp.rs` (Tool, Resource, etc.)
- `mcp-server/src/lib.rs` (JSON-RPC loop)

**Deliverables:**
- [ ] MCP protocol types in Hivemind
- [ ] JSON-RPC message loop
- [ ] Message processor routing
- [ ] Conversation lifecycle (create, send, interrupt)
- [ ] Approval request/response flow
- [ ] Event streaming

**Tests:**
- JSON-RPC message parsing
- Request routing
- Response serialization
- Event streaming

---

## Phase 4: User Interaction (Weeks 7-8, ~15 days)

### Structured User Input (2-3 days)
**Files to port:**
- `protocol/src/request_user_input.rs`
- `core/src/tools/handlers/request_user_input.rs`

**Deliverables:**
- [ ] `RequestUserInputQuestion` struct
- [ ] Handler implementation
- [ ] RPC endpoint
- [ ] Response routing

### Plan Management (1-2 days)
**Files to port:**
- `protocol/src/plan_tool.rs`
- `core/src/tools/handlers/plan.rs`

**Deliverables:**
- [ ] `UpdatePlanArgs` struct
- [ ] Handler implementation
- [ ] `PlanUpdate` event emission
- [ ] Plan state tracking

---

## Phase 5: Ecosystem (Weeks 9-10, ~15 days)

### Dynamic Tools (3-4 days)
**Files to port:**
- `protocol/src/dynamic_tools.rs`
- `app-server/src/dynamic_tools.rs`

**Deliverables:**
- [ ] `DynamicToolCallRequest` struct
- [ ] `DynamicToolResponse` struct
- [ ] Oneshot channel plumbing
- [ ] Schema validation
- [ ] RPC endpoints

### Advanced Protocol (2-3 days)
**Files to port:**
- `protocol/src/config_types.rs`
- `app-server-protocol/src/protocol/v2.rs`

**Deliverables:**
- [ ] `CollaborationMode` enum
- [ ] `ReasoningEffort` enum
- [ ] `Personality` enum
- [ ] Output schema validation
- [ ] Mode-specific tool gating

---

## Success Metrics

### Phase 1
- [ ] 0 silent policy bypasses (all decisions logged)
- [ ] Hook dispatch deterministic and testable
- [ ] Approval caching reduces redundant requests by 80%+

### Phase 2
- [ ] Session resumption works across restarts
- [ ] Metrics exported successfully
- [ ] p95 query latency < 100ms

### Phase 3
- [ ] MCP server handles 100+ concurrent connections
- [ ] Event streaming latency < 50ms
- [ ] Approval flow end-to-end working

### Phase 4
- [ ] User input questions render correctly
- [ ] Plan updates visible in UI
- [ ] No regressions in existing tool execution

### Phase 5
- [ ] Dynamic tools callable with schema validation
- [ ] Collaboration modes enforce tool gating
- [ ] Output schemas constrain model output

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| SQLite schema conflicts | Use migrations with version tracking |
| Hook process crashes | Timeout and error handling in dispatch |
| Approval routing bugs | Comprehensive unit tests + integration tests |
| MCP message loss | Bounded channels with backpressure |
| Metrics cardinality explosion | Tag validation and limits |

---

## Total Effort Estimate

- **Phase 1:** 25 days (foundation)
- **Phase 2:** 20 days (observability)
- **Phase 3:** 20 days (integration)
- **Phase 4:** 15 days (user interaction)
- **Phase 5:** 15 days (ecosystem)

**Total:** ~95 days (~19 weeks at 5 days/week)

**Recommended:** 2-3 engineers, 10-week timeline with parallel work on phases 1-2.

---

## Next Steps

1. Review this roadmap with team
2. Prioritize based on business needs
3. Allocate resources for Phase 1
4. Set up codex-rs as reference repository
5. Begin protocol type porting
6. Establish testing strategy

