# Hivemind Workflow Observability Guide

This directory contains scripts and documentation for debugging and monitoring Hivemind workflows.

## Quick Start

### 1. Find Your HIVEMIND Data Directory

```bash
# Check environment variable
echo $HIVEMIND_DATA_DIR

# Common locations
ls ~/.hivemind/                    # User-level
ls /tmp/hivemind-*/.hivemind/      # Project-level
```

### 2. Key Data Sources

| Source | Location | Purpose |
|--------|----------|---------|
| Events | `~/.hivemind/events.jsonl` | All workflow/task events |
| SQLite | `~/.hivemind/db.sqlite` | Structured state (tables: events) |
| opencode DB | `~/.opencode/data.db` | opencode session data |
| Worktrees | `~/.hivemind/worktrees/` | Per-flow worktrees |

## Event Analysis

### Query Events by Flow

```bash
# Count events for a flow
FLOW_ID="your-flow-id"
grep "$FLOW_ID" ~/.hivemind/events.jsonl | wc -l

# Group by event type
grep "$FLOW_ID" ~/.hivemind/events.jsonl | \
  python3 -c "
    import json, sys
    types = {}
    for line in sys.stdin:
      try:
        ev = json.loads(line)
        t = ev.get('payload', {}).get('type', 'unknown')
        types[t] = types.get(t, 0) + 1
      except: pass
    for t, c in sorted(types.items()):
      print(f'{t}: {c}')
  "
```

### Key Event Types

| Event | Meaning |
|-------|---------|
| `runtime_started` | Runtime began executing |
| `runtime_exited` | Runtime completed (check exit_code) |
| `runtime_terminated` | Runtime was terminated (check reason) |
| `checkpoint_completed` | Checkpoint saved |
| `all_checkpoints_completed` | All checkpoints done |
| `task_execution_state_changed` | Task moved states |
| `workflow_step_state_changed` | Step moved states |
| `error_occurred` | An error happened |

### Check Runtime Completion

```bash
# Find all runtime_exited events for a flow
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'runtime_exited':
        seq = ev.get('metadata', {}).get('sequence')
        attempt = p.get('attempt_id', 'N/A')[:8]
        exit_code = p.get('exit_code')
        print(f'seq={seq} attempt={attempt}... exit_code={exit_code}')
    except: pass
"
```

**Expected output for success:**
```
seq=1096 runtime_exited attempt=4ea2203f... exit_code=0
```

**Problem signs:**
- `exit_code=-1` - Runtime was killed/errored
- Multiple `runtime_exited` events for same attempt
- No `runtime_exited` but `runtime_terminated` with error

### Check Task State Transitions

```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'task_execution_state_changed':
        seq = ev.get('metadata', {}).get('sequence')
        from_state = p.get('from', '')
        to_state = p.get('to', '')
        task = p.get('task_id', 'N/A')[:20]
        print(f'seq={seq} {task}...: {from_state} → {to_state}')
    except: pass
"
```

### Check Step State Transitions

```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'workflow_step_state_changed':
        seq = ev.get('metadata', {}).get('sequence')
        state = p.get('state', '')
        step = p.get('step_id', 'N/A')[:20]
        print(f'seq={seq} {step}...: {state}')
    except: pass
"
```

### Check Runtime Output

```bash
# Find all runtime output chunks
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'runtime_output_chunk':
        seq = ev.get('metadata', {}).get('sequence')
        content = p.get('content', '')[:200]
        print(f'seq={seq}: {content}')
    except: pass
" | head -50
```

### Check Tool Calls

```bash
# Find tool call observations
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'runtime_tool_call_observed':
        seq = ev.get('metadata', {}).get('sequence')
        tool = p.get('tool_name', 'N/A')
        details = p.get('details', '')[:150]
        print(f'seq={seq} TOOL={tool}: {details}')
    except: pass
"
```

### Find Errors

```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      t = p.get('type')
      if t in ('error_occurred', 'task_execution_failed', 'workflow_run_aborted'):
        seq = ev.get('metadata', {}).get('sequence')
        reason = p.get('reason', p.get('message', ''))[:100]
        print(f'seq={seq} {t}: {reason}')
    except: pass
"
```

## opencode Session Analysis

### Find Session IDs

```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys, re
  sessions = set()
  for line in sys.stdin:
    try:
      content = line
      # Find session IDs in JSON payloads
      matches = re.findall(r'sessionID[\":]+([^\"\s,}]+)', content)
      for m in matches:
        sessions.add(m)
    except: pass
  for s in sorted(sessions):
    print(s)
"
```

### Query opencode SQLite DB

```bash
# Find opencode database
find ~/.opencode -name "*.db" -o -name "*.sqlite" 2>/dev/null

# Check tables
sqlite3 ~/.opencode/data.db ".tables"

# Query sessions
sqlite3 ~/.opencode/data.db "SELECT * FROM sessions WHERE id LIKE '%session_id%';"

# Query steps
sqlite3 ~/.opencode/data.db "SELECT * FROM steps WHERE session_id LIKE '%session_id%';"
```

## Timeline Analysis

### Build Complete Timeline

```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  events = []
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      seq = ev.get('metadata', {}).get('sequence')
      ts = ev.get('metadata', {}).get('timestamp', '')
      t = ev.get('payload', {}).get('type', 'unknown')
      events.append((seq, ts, t))
    except: pass
  events.sort()
  for seq, ts, t in events:
    print(f'{ts} seq={seq} {t}')
"
```

### Find Event Latencies

```bash
# Find time between runtime_exited and checkpoint_completed
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  events = {}
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      seq = ev.get('metadata', {}).get('sequence')
      ts = ev.get('metadata', {}).get('timestamp', '')
      t = p.get('type')
      if t in ('runtime_exited', 'checkpoint_completed', 'all_checkpoints_completed'):
        events[t] = (seq, ts)
    except: pass
  print(events)
"
```

## Common Issues

### Issue: No runtime_exited Event

**Symptoms:** Workflow stuck, no completion events

**Investigation:**
```bash
# Check if runtime ever started
grep "$FLOW_ID" ~/.hivemind/events.jsonl | \
  python3 -c "import json,sys; [print(l) for l in sys.stdin if 'runtime_started' in l]"
```

**Possible causes:**
1. Runtime never started - check `task_execution_started`
2. Runtime hanging - check `runtime_output_chunk` for recent activity
3. Process group issue - opencode may have zombie children

### Issue: Double runtime_exited Events

**Symptoms:** Two `runtime_exited` events, exit_code=0 then exit_code=-1

**Investigation:**
```bash
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'runtime_exited':
        seq = ev.get('metadata', {}).get('sequence')
        ec = p.get('exit_code')
        print(f'seq={seq} exit_code={ec}')
    except: pass
"
```

**Fix:** See `fix/opencode-runtime-completion-detection` branch

### Issue: Task Stuck in Running

**Symptoms:** Task shows `running` but no activity

**Investigation:**
```bash
# Check last events for task
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('task_id') == 'TARGET_TASK_ID':
        seq = ev.get('metadata', {}).get('sequence')
        t = p.get('type')
        print(f'seq={seq} {t}')
    except: pass
"
```

### Issue: Checkpoint Not Triggering Completion

**Symptoms:** `all_checkpoints_completed` but task doesn't complete

**Investigation:**
```bash
# Check exit_code in runtime_exited
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
  import json, sys
  for line in sys.stdin:
    try:
      ev = json.loads(line)
      p = ev.get('payload', {})
      if p.get('type') == 'runtime_exited':
        ec = p.get('exit_code')
        seq = ev.get('metadata', {}).get('sequence')
        print(f'seq={seq} exit_code={ec}')
    except: pass
"
```

**Expected:** `exit_code=0` for successful completion

## Scripts

### event-analysis.sh

Quick event summary for a flow:

```bash
#!/bin/bash
FLOW_ID=$1
EVENTS_FILE=${HIVEMIND_DATA_DIR:-~/.hivemind}/events.jsonl

echo "=== Event Summary for $FLOW_ID ==="
grep "$FLOW_ID" "$EVENTS_FILE" | wc -l
echo " events total"

echo ""
echo "=== Event Types ==="
grep "$FLOW_ID" "$EVENTS_FILE" | python3 -c "
import json, sys
types = {}
for line in sys.stdin:
  try:
    ev = json.loads(line)
    t = ev.get('payload', {}).get('type', 'unknown')
    types[t] = types.get(t, 0) + 1
  except: pass
for t, c in sorted(types.items()):
  print(f'  {t}: {c}')
"

echo ""
echo "=== Key Events ==="
grep "$FLOW_ID" "$EVENTS_FILE" | python3 -c "
import json, sys
for line in sys.stdin:
  try:
    ev = json.loads(line)
    p = ev.get('payload', {})
    t = p.get('type')
    if t in ('runtime_exited', 'runtime_terminated', 'checkpoint_completed', 'task_execution_failed', 'workflow_run_aborted'):
      seq = ev.get('metadata', {}).get('sequence')
      ec = p.get('exit_code', '')
      reason = p.get('reason', p.get('message', ''))[:60]
      print(f'seq={seq} {t} exit={ec} reason={reason}')
  except: pass
"
```

## Useful One-Liners

```bash
# Find all workflow runs for a project
grep "workflow_run_created" ~/.hivemind/events.jsonl | \
  python3 -c "import json,sys; [print(json.loads(l).get('payload',{}).get('workflow_run_id')) for l in sys.stdin if 'project_id' in l]"

# Find attempts for a flow
grep "$FLOW_ID" ~/.hivemind/events.jsonl | \
  python3 -c "import json,sys; print(set(json.loads(l).get('payload',{}).get('attempt_id')) for l in sys.stdin if 'attempt_id' in l)"

# Count runtime_output_chunk events
grep "$FLOW_ID" ~/.hivemind/events.jsonl | \
  grep -c "runtime_output_chunk"

# Find step state transitions to failed
grep "$FLOW_ID" ~/.hivemind/events.jsonl | python3 -c "
import json,sys
for line in sys.stdin:
  try:
    ev = json.loads(line)
    p = ev.get('payload', {})
    if p.get('type') == 'workflow_step_state_changed' and p.get('state') == 'failed':
      print(json.dumps(p, indent=2))
  except: pass
"
```

## Debugging Checklist

1. **Verify flow started:** `workflow_run_started` exists
2. **Verify task started:** `task_execution_started` exists
3. **Verify runtime started:** `runtime_started` exists
4. **Verify runtime exited:** `runtime_exited` with `exit_code=0`
5. **Verify checkpoints:** `all_checkpoints_completed` exists
6. **Verify task completed:** `task_execution_state_changed` to `success`
7. **Verify step completed:** `workflow_step_state_changed` to `succeeded`
8. **Check for errors:** `error_occurred`, `task_execution_failed`
9. **Check tool calls:** `runtime_tool_call_observed` shows what AI did
10. **Check output:** `runtime_output_chunk` shows AI's responses
