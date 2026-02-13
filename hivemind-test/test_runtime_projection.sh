#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"
export HOME="$TMP_BASE/.hm_home_projection"
rm -rf "$HOME"

rm -rf "$TMP_BASE/test-runtime-projection"
mkdir -p "$TMP_BASE/test-runtime-projection"
cd "$TMP_BASE/test-runtime-projection"

git init >/dev/null
echo "# Runtime Projection Test" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit" >/dev/null

echo "=== Creating project ==="
$HIVEMIND project create "runtime-projection" --description "Runtime projection smoke test" >/dev/null

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo runtime-projection "$PWD" --name main >/dev/null

echo "=== Configuring runtime ==="
$HIVEMIND project runtime-set runtime-projection \
  --adapter opencode \
  --binary-path /usr/bin/env \
  --arg sh \
  --arg -c \
  --arg "echo '$ cargo test'; echo 'Tool: grep'; echo '- [ ] collect logs'; echo '- [x] collect logs'; echo 'I will verify outputs'; echo 'Command: cargo clippy' 1>&2; printf data > projection.txt" \
  --timeout-ms 1000 >/dev/null

echo "=== Creating task/graph/flow ==="
TASK_ID=$($HIVEMIND task create runtime-projection "projection-task" 2>&1 | grep "ID:" | head -1 | awk '{print $2}')
GRAPH_ID=$($HIVEMIND graph create runtime-projection "projection-graph" --from-tasks "$TASK_ID" 2>&1 | grep "Graph ID:" | awk '{print $3}')
FLOW_ID=$($HIVEMIND flow create "$GRAPH_ID" 2>&1 | grep "Flow ID:" | awk '{print $3}')

$HIVEMIND flow start "$FLOW_ID" >/dev/null
set +e
TICK_OUT=$($HIVEMIND flow tick "$FLOW_ID" 2>&1)
TICK_RC=$?
set -e
echo "$TICK_OUT"

if [ "$TICK_RC" -ne 0 ]; then
  if echo "$TICK_OUT" | grep -q "checkpoints_incomplete"; then
    EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 500)"
    ATTEMPT_ID="$(echo "$EVENTS_JSON" | sed -n 's/.*"attempt_id": "\([0-9a-f-]\{36\}\)".*/\1/p' | head -n1)"
    if [ -z "$ATTEMPT_ID" ]; then
      echo "failed to discover attempt_id from events" >&2
      exit 1
    fi
    $HIVEMIND checkpoint complete --attempt-id "$ATTEMPT_ID" --id checkpoint-1 --summary "runtime projection checkpoint" >/dev/null
    $HIVEMIND task complete "$TASK_ID" >/dev/null
    $HIVEMIND flow tick "$FLOW_ID" >/dev/null
  else
    echo "$TICK_OUT" >&2
    exit "$TICK_RC"
  fi
fi

echo "=== Checking projected runtime events ==="
EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 200)"

echo "$EVENTS_JSON" | grep -q 'runtime_command_observed'
echo "$EVENTS_JSON" | grep -q 'runtime_tool_call_observed'
echo "$EVENTS_JSON" | grep -q 'runtime_todo_snapshot_updated'
echo "$EVENTS_JSON" | grep -q 'runtime_narrative_output_observed'

echo "Projection events found."
