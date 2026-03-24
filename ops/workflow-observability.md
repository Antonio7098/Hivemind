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
| SQLite | `~/.hivemind/db.sqlite` | **Canonical event storage** (tables: events) |
| opencode DB | `~/.opencode/data.db` | opencode session data |
| Worktrees | `~/.hivemind/worktrees/` | Per-flow worktrees |

## Event Analysis

### Query Events by Flow

```bash
# Count events for a flow
FLOW_ID="your-flow-id"
sqlite3 ~/.hivemind/db.sqlite "SELECT COUNT(*) FROM events WHERE flow_id = '$FLOW_ID';"

# Group by event type
sqlite3 ~/.hivemind/db.sqlite "
SELECT json_extract(event_json, '$.payload.type') as event_type, COUNT(*) as count 
FROM events 
WHERE flow_id = '$FLOW_ID'
GROUP BY event_type 
ORDER BY count DESC;
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
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, 
       substr(attempt_id, 1, 8) || '...' as attempt,
       json_extract(event_json, '$.payload.exit_code') as exit_code
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_exited'
ORDER BY sequence;
"
```

**Expected output for success:**
```
1096|4ea2203f...|0
```

**Problem signs:**
- `exit_code=-1` - Runtime was killed/errored
- Multiple `runtime_exited` events for same attempt
- No `runtime_exited` but `runtime_terminated` with error

### Check Task State Transitions

```bash
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence,
       substr(task_id, 1, 20) || '...' as task,
       json_extract(event_json, '$.payload.from') as from_state,
       json_extract(event_json, '$.payload.to') as to_state
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'task_execution_state_changed'
ORDER BY sequence;
"
```

### Check Step State Transitions

```bash
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence,
       substr(step_id, 1, 20) || '...' as step,
       json_extract(event_json, '$.payload.state') as state
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'workflow_step_state_changed'
ORDER BY sequence;
"
```

### Check Runtime Output

```bash
# Find all runtime output chunks
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, substr(json_extract(event_json, '$.payload.content'), 1, 200) as content
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_output_chunk'
ORDER BY sequence
LIMIT 50;
"
```

### Check Tool Calls

```bash
# Find tool call observations
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, json_extract(event_json, '$.payload.name') as tool_name
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'tool_call_observation'
ORDER BY sequence;
      "
```

### Find Errors

```bash
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, 
       json_extract(event_json, '$.payload.type') as error_type,
       substr(json_extract(event_json, '$.payload.reason'), 1, 100) as reason
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') IN ('error_occurred', 'task_execution_failed', 'workflow_run_aborted')
ORDER BY sequence;
"
```

## opencode Session Analysis

### Find Session IDs

```bash
sqlite3 ~/.hivemind/db.sqlite "
SELECT DISTINCT substr(json_extract(event_json, '$.payload.sessionId'), 1, 36) as session_id
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.sessionId') IS NOT NULL;
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
sqlite3 ~/.hivemind/db.sqlite "
SELECT timestamp_rfc3339, sequence, json_extract(event_json, '$.payload.type') as event_type
FROM events 
WHERE flow_id = '$FLOW_ID'
ORDER BY sequence;
"
```

### Find Event Latencies

```bash
# Find time between runtime_exited and checkpoint_completed
sqlite3 ~/.hivemind/db.sqlite "
WITH runtime_events AS (
  SELECT timestamp_rfc3339, sequence, json_extract(event_json, '$.payload.type') as type
  FROM events 
  WHERE flow_id = '$FLOW_ID' 
    AND json_extract(event_json, '$.payload.type') IN ('runtime_exited', 'checkpoint_completed', 'all_checkpoints_completed')
),
timed_events AS (
  SELECT type, timestamp_rfc3339,
         LAG(timestamp_rfc3339) OVER (PARTITION BY type ORDER BY sequence) as prev_timestamp
  FROM runtime_events
)
SELECT type, timestamp_rfc3339, prev_timestamp,
       (julianday(timestamp_rfc3339) - julianday(prev_timestamp)) * 86400 as seconds_delta
FROM timed_events 
WHERE prev_timestamp IS NOT NULL
ORDER BY timestamp_rfc3339;
"
```

## Common Issues

### Issue: No runtime_exited Event

**Symptoms:** Workflow stuck, no completion events

**Investigation:**
```bash
# Check if runtime ever started
sqlite3 ~/.hivemind/db.sqlite "
SELECT COUNT(*) as runtime_started_count
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_started';
"
```

**Possible causes:**
1. Runtime never started - check `task_execution_started`
2. Runtime hanging - check `runtime_output_chunk` for recent activity
3. Process group issue - opencode may have zombie children

### Issue: Double runtime_exited Events

**Symptoms:** Two `runtime_exited` events, exit_code=0 then exit_code=-1

**Investigation:**
```bash
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, json_extract(event_json, '$.payload.exit_code') as exit_code
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_exited'
ORDER BY sequence;
"
```

**Fix:** See `fix/opencode-runtime-completion-detection` branch

### Issue: Task Stuck in Running

**Symptoms:** Task shows `running` but no activity

**Investigation:**
```bash
# Check last events for task
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, json_extract(event_json, '$.payload.type') as event_type
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND task_id = 'TARGET_TASK_ID'
ORDER BY sequence DESC
LIMIT 10;
"
```

### Issue: Checkpoint Not Triggering Completion

**Symptoms:** `all_checkpoints_completed` but task doesn't complete

**Investigation:**
```bash
# Check exit_code in runtime_exited
sqlite3 ~/.hivemind/db.sqlite "
SELECT sequence, json_extract(event_json, '$.payload.exit_code') as exit_code
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_exited'
ORDER BY sequence;
"
```

**Expected:** `exit_code=0` for successful completion

## Scripts

### event-analysis.sh

Quick event summary for a flow:

```bash
#!/bin/bash
FLOW_ID=$1
DB_PATH=${HIVEMIND_DATA_DIR:-~/.hivemind}/db.sqlite

echo "=== Event Summary for $FLOW_ID ==="
sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM events WHERE flow_id = '$FLOW_ID';"
echo " events total"

echo ""
echo "=== Event Types ==="
sqlite3 "$DB_PATH" "
SELECT json_extract(event_json, '$.payload.type') as event_type, COUNT(*) as count
FROM events 
WHERE flow_id = '$FLOW_ID'
GROUP BY event_type 
ORDER BY count DESC;
"

echo ""
echo "=== Key Events ==="
sqlite3 "$DB_PATH" "
SELECT sequence, 
       json_extract(event_json, '$.payload.type') as event_type,
       json_extract(event_json, '$.payload.exit_code') as exit_code,
       substr(json_extract(event_json, '$.payload.reason'), 1, 60) as reason
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') IN ('runtime_exited', 'runtime_terminated', 'checkpoint_completed', 'task_execution_failed', 'workflow_run_aborted')
ORDER BY sequence;
"
```

## Useful One-Liners

```bash
# Find all workflow runs for a project
sqlite3 ~/.hivemind/db.sqlite "
SELECT DISTINCT json_extract(event_json, '$.payload.workflow_run_id') as workflow_run_id
FROM events 
WHERE json_extract(event_json, '$.payload.type') = 'workflow_run_created'
  AND json_extract(event_json, '$.payload.project_id') = 'PROJECT_ID';
"

# Find attempts for a flow
sqlite3 ~/.hivemind/db.sqlite "
SELECT DISTINCT json_extract(event_json, '$.payload.attempt_id') as attempt_id
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.attempt_id') IS NOT NULL;
"

# Count runtime_output_chunk events
sqlite3 ~/.hivemind/db.sqlite "
SELECT COUNT(*) 
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'runtime_output_chunk';
"

# Find step state transitions to failed
sqlite3 ~/.hivemind/db.sqlite "
SELECT event_json
FROM events 
WHERE flow_id = '$FLOW_ID' 
  AND json_extract(event_json, '$.payload.type') = 'workflow_step_state_changed'
  AND json_extract(event_json, '$.payload.state') = 'failed';
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
