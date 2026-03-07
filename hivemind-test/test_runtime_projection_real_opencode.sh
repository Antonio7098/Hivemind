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
TASK_ID=$($HIVEMIND task create real-runtime-projection "$PROMPT" 2>&1 | awk '/ID:/ {print $2; exit}')
GRAPH_ID=$($HIVEMIND graph create real-runtime-projection "real-projection-graph" --from-tasks "$TASK_ID" 2>&1 | awk '/Graph ID:/ {print $3; exit}')
FLOW_ID=$($HIVEMIND flow create "$GRAPH_ID" 2>&1 | awk '/Flow ID:/ {print $3; exit}')

$HIVEMIND flow start "$FLOW_ID" >/dev/null

# First tick runs the runtime and may stop at checkpoints_incomplete.
set +e
$HIVEMIND flow tick "$FLOW_ID" >/dev/null
TICK_EXIT=$?
set -e

if [ "$TICK_EXIT" -ne 0 ]; then
  echo "flow tick returned non-zero (expected if checkpoint completion is pending): $TICK_EXIT"
fi

EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 500)"

if [[ "$EVENTS_JSON" == *'"type": "checkpoint_completed"'* ]]; then
  echo "checkpoint already completed by runtime"
else
  ATTEMPT_ID="$(EVENTS_JSON="$EVENTS_JSON" python3 - <<'PY'
import os
import re

match = re.search(r'"attempt_id": "([0-9a-f-]{36})"', os.environ.get("EVENTS_JSON", ""))
print(match.group(1) if match else "")
PY
)"

  if [ -z "$ATTEMPT_ID" ]; then
    echo "failed to discover attempt_id from events"
    exit 1
  fi

  echo "=== Completing checkpoint ==="
  $HIVEMIND checkpoint complete --attempt-id "$ATTEMPT_ID" --id checkpoint-1 --summary "real runtime projection checkpoint" >/dev/null

  echo "=== Finalizing attempt after checkpoint completion ==="
  $HIVEMIND flow tick "$FLOW_ID" >/dev/null
fi

echo "=== Validating projected runtime events from real opencode output ==="
EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 500)"

[[ "$EVENTS_JSON" == *"runtime_started"* ]]
[[ "$EVENTS_JSON" == *"runtime_output_chunk"* ]]
[[ "$EVENTS_JSON" == *"runtime_exited"* ]]

PROJECTED_COUNT=0
for EVENT_TYPE in \
  runtime_command_observed \
  runtime_tool_call_observed \
  runtime_todo_snapshot_updated \
  runtime_narrative_output_observed
do
  if [[ "$EVENTS_JSON" == *"\"type\": \"$EVENT_TYPE\""* ]]; then
    echo "observed projection: $EVENT_TYPE"
    PROJECTED_COUNT=$((PROJECTED_COUNT + 1))
  fi
done

if [[ "$EVENTS_JSON" != *"runtime_command_observed"* ]]; then
  echo "expected at least one runtime_command_observed event from real opencode output"
  exit 1
fi

if [ "$PROJECTED_COUNT" -eq 0 ]; then
  echo "expected at least one projected runtime observation from real opencode output"
  exit 1
fi

echo "=== Runtime projection event excerpt ==="
awk '/runtime_(command_observed|tool_call_observed|todo_snapshot_updated|narrative_output_observed)/ {print; c++; if (c>=8) exit}' <<<"$EVENTS_JSON"

echo "Real opencode projection events found."
