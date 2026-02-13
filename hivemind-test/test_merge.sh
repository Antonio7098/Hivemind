#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"

TMP_BASE="${TMPDIR:-/tmp}/hivemind-test"

export HOME="$TMP_BASE/.hm_home"
rm -rf "$HOME"

rm -rf "$TMP_BASE/test-merge"
mkdir -p "$TMP_BASE/test-merge"
cd "$TMP_BASE/test-merge"

git init

echo "# Test Project" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"
git branch -M main

echo "=== Creating project ==="
PROJECT_OUTPUT=$($HIVEMIND project create "test-merge" --description "Test project" 2>&1)
echo "$PROJECT_OUTPUT"
PROJECT_ID=$(echo "$PROJECT_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo test-merge "$PWD" --name "main" 2>&1

echo "=== Configuring runtime ==="
$HIVEMIND project runtime-set test-merge \
  --adapter opencode \
  --binary-path /usr/bin/env \
  --arg sh \
  --arg -c \
  --arg "printf 'Hello, Merge!' > hello.txt" \
  --timeout-ms 1000 2>&1

echo "=== Creating task ==="
TASK_OUTPUT=$($HIVEMIND task create test-merge "Create hello.txt" --description "Create hello.txt with content 'Hello, Merge!'" 2>&1)
echo "$TASK_OUTPUT"
TASK_ID=$(echo "$TASK_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')

echo "=== Creating graph ==="
GRAPH_OUTPUT=$($HIVEMIND graph create test-merge "merge-graph" --from-tasks "$TASK_ID" 2>&1)
echo "$GRAPH_OUTPUT"
GRAPH_ID=$(echo "$GRAPH_OUTPUT" | grep "Graph ID:" | awk '{print $3}')

echo "=== Adding required check ==="
$HIVEMIND graph add-check "$GRAPH_ID" "$TASK_ID" --name "hello_exists" --command "test -f hello.txt" --required 2>&1

echo "=== Creating flow ==="
FLOW_OUTPUT=$($HIVEMIND flow create "$GRAPH_ID" 2>&1)
echo "$FLOW_OUTPUT"
FLOW_ID=$(echo "$FLOW_OUTPUT" | grep "Flow ID:" | awk '{print $3}')

echo "=== Starting flow ==="
$HIVEMIND flow start "$FLOW_ID" 2>&1

echo "=== Ticking flow until completed ==="
for i in $(seq 1 10); do
  set +e
  TICK_OUT=$($HIVEMIND flow tick "$FLOW_ID" 2>&1)
  TICK_RC=$?
  set -e
  echo "$TICK_OUT"

  if [ "$TICK_RC" -ne 0 ]; then
    if echo "$TICK_OUT" | grep -q "checkpoints_incomplete"; then
      echo "=== Completing checkpoint and retrying tick ==="
      EVENTS_JSON="$($HIVEMIND -f json events stream --flow "$FLOW_ID" --limit 500)"
      ATTEMPT_ID="$(echo "$EVENTS_JSON" | sed -n 's/.*"attempt_id": "\([0-9a-f-]\{36\}\)".*/\1/p' | head -n1)"
      if [ -z "$ATTEMPT_ID" ]; then
        echo "Failed to find attempt_id for checkpoint completion" >&2
        exit 1
      fi
      $HIVEMIND checkpoint complete --attempt-id "$ATTEMPT_ID" --id checkpoint-1 --summary "legacy merge script" 2>&1
      $HIVEMIND task complete "$TASK_ID" 2>&1
      $HIVEMIND flow tick "$FLOW_ID" 2>&1
    else
      echo "$TICK_OUT" >&2
      exit "$TICK_RC"
    fi
  fi

  STATUS_OUT=$($HIVEMIND flow status "$FLOW_ID" 2>&1)
  echo "$STATUS_OUT"
  if echo "$STATUS_OUT" | grep -q "State:" && echo "$STATUS_OUT" | grep -q "Completed"; then
    break
  fi
  sleep 0.1
done

FINAL_STATUS=$($HIVEMIND flow status "$FLOW_ID" 2>&1)
echo "$FINAL_STATUS"
if ! echo "$FINAL_STATUS" | grep -q "Completed"; then
  echo "Flow did not reach Completed state; cannot proceed to merge" >&2
  exit 1
fi

echo "=== Merge prepare ==="
$HIVEMIND merge prepare "$FLOW_ID" --target main 2>&1

echo "=== Verifying flow is frozen for merge ==="
FROZEN_STATUS=$($HIVEMIND flow status "$FLOW_ID" 2>&1)
echo "$FROZEN_STATUS"
if ! echo "$FROZEN_STATUS" | grep -q "FrozenForMerge"; then
  echo "Flow did not transition to FrozenForMerge after merge prepare" >&2
  exit 1
fi

echo "=== Merge approve ==="
$HIVEMIND merge approve "$FLOW_ID" 2>&1

echo "=== Merge execute ==="
$HIVEMIND merge execute "$FLOW_ID" 2>&1

echo "=== Verifying flow is merged ==="
MERGED_STATUS=$($HIVEMIND flow status "$FLOW_ID" 2>&1)
echo "$MERGED_STATUS"
if ! echo "$MERGED_STATUS" | grep -q "Merged"; then
  echo "Flow did not transition to Merged after merge execute" >&2
  exit 1
fi

echo "=== Verifying tight merge protocol events ==="
EVENTS_OUT=$($HIVEMIND events list --limit 500 2>&1)
echo "$EVENTS_OUT"

for event in \
  task_execution_frozen \
  flow_frozen_for_merge \
  flow_integration_lock_acquired \
  task_integrated_into_flow \
  merge_prepared \
  merge_approved \
  merge_completed
do
  if ! echo "$EVENTS_OUT" | grep -q "$event"; then
    echo "Expected event type '$event' not found in event stream" >&2
    exit 1
  fi
done

echo "=== Verifying merged result on main ==="
git checkout main 2>&1

test -f hello.txt
cat hello.txt

echo "=== Test complete ==="
