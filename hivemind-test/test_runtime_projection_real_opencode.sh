#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"
OPENCODE_BIN="${OPENCODE_BIN:-/home/antonio/.opencode/bin/opencode}"
OPENCODE_MODEL="${OPENCODE_MODEL:-opencode/big-pickle}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"
export HOME="$TMP_BASE/.hm_home_real_projection"
rm -rf "$HOME"

rm -rf "$TMP_BASE/test-real-runtime-projection"
mkdir -p "$TMP_BASE/test-real-runtime-projection"
cd "$TMP_BASE/test-real-runtime-projection"

git init >/dev/null
echo "# Real Runtime Projection Test" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit" >/dev/null

echo "=== Creating project ==="
$HIVEMIND project create "real-runtime-projection" --description "Real opencode runtime projection validation" >/dev/null

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo real-runtime-projection "$PWD" --name main >/dev/null

echo "=== Configuring real opencode runtime ==="
$HIVEMIND project runtime-set real-runtime-projection \
  --adapter opencode \
  --binary-path "$OPENCODE_BIN" \
  --arg run \
  --arg -m \
  --arg "$OPENCODE_MODEL" \
  --arg --format \
  --arg default \
  --timeout-ms 120000 >/dev/null

PROMPT="Output exactly these five lines and nothing else:\n$ cargo test\nTool: grep\n- [ ] collect logs\n- [x] collect logs\nI will verify outputs now."

echo "=== Creating task/graph/flow ==="
TASK_ID=$($HIVEMIND task create real-runtime-projection "$PROMPT" 2>&1 | grep "ID:" | head -1 | awk '{print $2}')
GRAPH_ID=$($HIVEMIND graph create real-runtime-projection "real-projection-graph" --from-tasks "$TASK_ID" 2>&1 | grep "Graph ID:" | awk '{print $3}')
FLOW_ID=$($HIVEMIND flow create "$GRAPH_ID" 2>&1 | grep "Flow ID:" | awk '{print $3}')

$HIVEMIND flow start "$FLOW_ID" >/dev/null
$HIVEMIND flow tick "$FLOW_ID" >/dev/null

echo "=== Validating projected runtime events from real opencode output ==="
EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 500)"

echo "$EVENTS_JSON" | grep -q 'runtime_started'
echo "$EVENTS_JSON" | grep -q 'runtime_output_chunk'
echo "$EVENTS_JSON" | grep -q 'runtime_exited'
echo "$EVENTS_JSON" | grep -q 'runtime_command_observed'
echo "$EVENTS_JSON" | grep -q 'runtime_tool_call_observed'
echo "$EVENTS_JSON" | grep -q 'runtime_todo_snapshot_updated'
echo "$EVENTS_JSON" | grep -q 'runtime_narrative_output_observed'

echo "=== Runtime projection event excerpt ==="
echo "$EVENTS_JSON" | grep -E 'runtime_(command_observed|tool_call_observed|todo_snapshot_updated|narrative_output_observed)' | head -n 8

echo "Real opencode projection events found."
