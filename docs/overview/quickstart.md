---
title: Quickstart Guide
description: Get started with Hivemind
order: 3
---

# Hivemind Quickstart (Humans + LLM Operators)

This is a copy/paste guide for running Hivemind end-to-end on a real repository using a real agent runtime (example: `opencode`).

Hivemind is **local-first** and **event-sourced**:

- All orchestration state lives in `events.jsonl` under `HIVEMIND_DATA_DIR` (default: `~/.hivemind`).
- All derived state is reconstructed from events.
- Code changes happen in **git worktrees/branches** owned by the flow engine.

---

## 0) Prerequisites

- Rust toolchain installed (`cargo`, `rustc`).
- Git installed.
- A git repository you want to operate on.
- A runtime adapter binary installed (example: `opencode`).

---

## 1) Build and run Hivemind

From the Hivemind repo:

```bash
cargo build --release
./target/release/hivemind version
```

If you are integrating Hivemind into scripts, prefer JSON output:

```bash
./target/release/hivemind -f json version
```

For shell scripting, it helps to have a small helper to extract IDs from JSON.
Some commands print IDs as top-level fields (e.g. `{ "task_id": "..." }`), while others print a response wrapper (e.g. `{ "success": true, "data": { ... } }`).

```bash
hm_id() {
  python3 - <<'PY'
import json,sys
obj=json.load(sys.stdin)
if isinstance(obj, dict) and "data" in obj and isinstance(obj["data"], dict):
  obj=obj["data"]

for k in ("project_id","task_id","graph_id","flow_id","attempt_id"):
  if isinstance(obj, dict) and k in obj:
    print(obj[k])
    raise SystemExit(0)

raise SystemExit("no known id field found")
PY
}
```

---

## 2) Choose a Hivemind data directory (recommended)

Hivemind defaults to `~/.hivemind`. For reproducible runs and tests, set `HIVEMIND_DATA_DIR`:

```bash
export HIVEMIND_DATA_DIR="/tmp/hivemind-demo/.hivemind"
mkdir -p "$HIVEMIND_DATA_DIR"
```

**What goes in `HIVEMIND_DATA_DIR`:**

- `events.jsonl` (append-only event log)
- artifacts (baselines, diffs)

---

## 3) Create a project + attach a repository

```bash
HM=./target/release/hivemind

$HM project create "expense-tracker" --description "Demo project"
$HM project attach-repo "expense-tracker" /absolute/path/to/your/repo --access rw
```

Tip: if you have multiple projects, use JSON output and capture IDs:

```bash
$HM -f json project list
$HM -f json project inspect "expense-tracker"
```

---

## 4) Configure the runtime (example: OpenCode)

### 4.1 Verify the runtime binary is runnable

```bash
/absolute/path/to/opencode --version
```

### 4.2 Configure runtime in Hivemind

```bash
$HM project runtime-set "expense-tracker" \
  --role worker \
  --adapter opencode \
  --binary-path /absolute/path/to/opencode \
  --model opencode/kimi-k2.5-free \
  --max-parallel-tasks 2 \
  --timeout-ms 600000
```

Notes:

- `--model` is optional; if omitted, the adapter may use its own default.
- `--max-parallel-tasks` controls per-project scheduling concurrency policy.
- If the runtime requires credentials, set them in your environment (do not hardcode secrets in scripts).
- Runtime resolution precedence is: `task override -> flow default -> project default -> global default`.

Optional global fallback defaults:

```bash
$HM runtime defaults-set \
  --role worker \
  --adapter opencode \
  --binary-path /absolute/path/to/opencode
```

Optional runtime diagnostics:

```bash
$HM runtime list
$HM runtime health --project "expense-tracker"
```

Optional task-level override (uses task runtime instead of project default):

```bash
$HM task runtime-set "$TASK_A" \
  --role worker \
  --adapter kilo \
  --binary-path /absolute/path/to/kilo \
  --model opencode/kimi-k2.5-free
```

---

## 5) Create tasks

Create tasks with:

- a **title** (short)
- a **description** (detailed)
- optional **scope** (what files it is allowed to touch)

```bash
TASK_A=$($HM -f json task create "expense-tracker" "Add CLI skeleton" --description "Create expense_tracker.py with argparse and subcommands" | hm_id)
TASK_B=$($HM -f json task create "expense-tracker" "Add add/list commands" --description "Implement add/list operations with JSON storage" | hm_id)
TASK_C=$($HM -f json task create "expense-tracker" "Add total/export" --description "Implement total + export (csv/json)" | hm_id)
```

If you prefer, you can still use `jq` (when the ID is top-level): `jq -r .task_id`.

---

## 6) Create a graph + dependencies

### 6.1 Create a graph

```bash
GRAPH_ID=$($HM -f json graph create "expense-tracker" "expense-tracker-v1" --from-tasks "$TASK_A" "$TASK_B" "$TASK_C" | hm_id)
```

### 6.2 Add dependencies (IMPORTANT direction)

`graph add-dependency <graph> <from_task> <to_task>` means:

- **`from_task` depends on `to_task`**
- `from_task` cannot run until `to_task` is complete

So if B needs A, and C needs B:

```bash
$HM graph add-dependency "$GRAPH_ID" "$TASK_B" "$TASK_A"
$HM graph add-dependency "$GRAPH_ID" "$TASK_C" "$TASK_B"
```

---

## 7) Create and run a flow

### 7.1 Create the flow

```bash
FLOW_ID=$($HM -f json flow create "$GRAPH_ID" --name "expense-tracker-beta" | hm_id)
```

### 7.2 Choose run mode (manual or auto)

Manual mode (default) requires explicit `flow start` and `flow tick`:

```bash
$HM flow set-run-mode "$FLOW_ID" manual
```

Auto mode progresses when dependencies are met:

```bash
$HM flow set-run-mode "$FLOW_ID" auto
```

You can also control per-task scheduling:

```bash
$HM task set-run-mode "$TASK_B" manual
```

### 7.3 Start it (manual flow mode)

```bash
$HM flow start "$FLOW_ID"
```

### 7.4 Tick it until it completes (manual flow mode)

Hivemind is deterministic and explicit: you advance execution using `flow tick`.
For concurrency governance, optionally set a global cap:

```bash
export HIVEMIND_MAX_PARALLEL_TASKS_GLOBAL=4
```

```bash
while true; do
  STATE=$($HM -f json flow status "$FLOW_ID" | python3 -c 'import json,sys; print(json.load(sys.stdin)["data"]["state"])')
  echo "flow state: $STATE"

  if [ "$STATE" = "completed" ] || [ "$STATE" = "aborted" ]; then
    break
  fi

  $HM flow tick "$FLOW_ID" --max-parallel 2
  sleep 1
done
```

### 7.5 Inter-flow dependencies

To make a downstream flow wait for an upstream flow:

```bash
UPSTREAM_FLOW_ID=$($HM -f json flow create "$GRAPH_ID" --name "upstream" | hm_id)
DOWNSTREAM_FLOW_ID=$($HM -f json flow create "$GRAPH_ID" --name "downstream" | hm_id)
$HM flow add-dependency "$DOWNSTREAM_FLOW_ID" "$UPSTREAM_FLOW_ID"
$HM flow set-run-mode "$DOWNSTREAM_FLOW_ID" auto
$HM flow set-run-mode "$UPSTREAM_FLOW_ID" auto
```

When the upstream flow completes, Hivemind automatically starts the downstream flow (if it is `auto`).

---

## 8) Inspect what happened (events, attempts, worktrees)

### 8.1 Event stream

```bash
$HM -f json events stream --flow "$FLOW_ID" --limit 200
$HM -f json events stream --flow "$FLOW_ID" --since "2026-01-01T00:00:00Z" --limit 200
$HM -f json events list --flow "$FLOW_ID" --since "2026-01-01T00:00:00Z" --until "2026-12-31T23:59:59Z" --limit 500
```

You should see runtime lifecycle events like:

- `runtime_started`
- `runtime_output_chunk`
- `runtime_filesystem_observed`
- `runtime_exited`

### 8.2 Attempts

```bash
$HM -f json attempt inspect <attempt-id> --context --diff --output
```

### 8.3 Worktrees

```bash
$HM -f json worktree list "$FLOW_ID"
$HM -f json worktree inspect <task-id>
$HM -f json worktree cleanup "$FLOW_ID" --dry-run
$HM -f json worktree cleanup "$FLOW_ID" --force
```

---

## 9) Merge workflow (explicit, gated)

When a flow is `completed`, merge is a separate, explicit protocol.

1) Prepare

```bash
# Use --target to explicitly select the integration target branch.
# This matters if your repo default branch is not "main".
$HM -f json merge prepare "$FLOW_ID" --target master
```

2) Approve

```bash
$HM -f json merge approve "$FLOW_ID"
```

3) Execute

```bash
$HM -f json merge execute "$FLOW_ID" --mode local
```

Or execute via GitHub PR automation:

```bash
$HM -f json merge execute "$FLOW_ID" \
  --mode pr \
  --monitor-ci \
  --auto-merge \
  --pull-after
```

Preconditions for `merge execute`:

- Repo must be clean (Hivemind ignores `.hivemind/` changes).
- Merge must be prepared and approved.

---

## 10) Troubleshooting (common sticking points)

### 10.1 “default branch” problems (`main` vs `master`)

If `merge execute` merges into the wrong branch, re-run `merge prepare` with an explicit `--target`.

### 10.2 Git identity / commit errors during merge

Hivemind uses an explicit identity for integration commits.
If your environment enforces strict git config policies, set repo-local identity:

```bash
git -C /path/to/repo config user.name "Hivemind"
git -C /path/to/repo config user.email "hivemind@example.com"
```

### 10.3 Flow appears “stuck”

- Check `flow status` to see whether tasks are `ready`, `running`, `verifying`, etc.
- Stream events for the flow and look for `runtime_output_chunk` / `runtime_exited`.

### 10.4 JSON parsing errors in scripts

Prefer `-f json` and validate that stdout is non-empty before parsing.
If you are mixing logs with JSON, write logs to stderr and parse stdout only.

### 10.5 Worktree inspection shows `is_worktree: false`

This typically means the worktree directory does not exist (it may have been cleaned up) or the repo is not in the expected state.
Use `worktree list` to find expected paths.

---

## Appendix: LLM operator rules of engagement

If you are an LLM operating Hivemind:

- Always use `-f json` for machine-readability.
- Treat the event log (`events.jsonl`) as the source of truth.
- Never assume dependency direction; follow the rule:
  - `add-dependency(from, to)` means **from depends on to**.
- Do not mutate execution branches manually while a flow is running.
- If merge preparation reports conflicts, stop and surface the conflict list.
- Prefer explicit branch targets for merges (`--target`).
