# Registry Refactor Checklist

- [x] Core struct + basic project APIs (src/core/registry/registry.rs)
- [x] Task APIs migrated (src/core/registry/tasks.rs)
- [x] Runtime module migrated (src/core/registry/runtime.rs)
- [x] Flow/attempts module migrated (src/core/registry/flow.rs)
- [x] Governance module migrated (src/core/registry/governance.rs)
- [x] Templates/global artifacts module migrated (src/core/registry/templates.rs)
- [x] Worktree management module migrated (src/core/registry/worktree.rs)
- [x] Events module migrated (src/core/registry/events.rs)
- [x] Context assembly module migrated (src/core/registry/context.rs)
- [x] Graph module migrated (src/core/registry/graph.rs)
- [x] CLI handlers wired to new modules
- [ ] Server references updated (full audit pending)
- [x] Original registry file removed
- [x] `cargo fmt`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo test`

## Next Split Targets (Post-Registry)

- [ ] src/native/tool_engine.rs (~5110 LOC)
- [ ] src/server.rs (~2089 LOC)
- [ ] src/cli/commands.rs (~1893 LOC)
- [ ] src/storage/event_store.rs (~1597 LOC)
- [ ] src/native/runtime_hardening.rs (~1538 LOC)
- [ ] src/core/state.rs (~1466 LOC)
- [ ] src/core/events.rs (~1460 LOC)

## Tooling

- [x] Added function dependency graph script (`scripts/rust_fn_dependency_graph.py`)
- [x] Added generated dependency reports (`ops/migration_graphs/*.deps.json`)
- [ ] Extend script with auto-extract/apply for method blocks (`impl` aware rewrite)
