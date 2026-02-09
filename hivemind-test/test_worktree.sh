#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-$REPO_ROOT/target/release/hivemind}"

export HOME="$SCRIPT_DIR/.hm_home"
rm -rf "$HOME"

echo "=== Creating fresh test ==="
rm -rf "$SCRIPT_DIR/test-fresh"
mkdir -p "$SCRIPT_DIR/test-fresh"
cd "$SCRIPT_DIR/test-fresh"
git init

# Create initial commit
echo "# Test Project" > README.md
git add README.md
git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

echo "=== Creating project ==="
$HIVEMIND project create "test-fresh" --description "Test project" 2>&1

echo "=== Attaching repository ==="
$HIVEMIND project attach-repo test-fresh "$PWD" --name "main" 2>&1

echo "=== Creating task ==="
TASK_OUTPUT=$($HIVEMIND task create test-fresh "Test task" --description "A test task" 2>&1)
echo "$TASK_OUTPUT"
TASK_ID=$(echo "$TASK_OUTPUT" | grep "ID:" | head -1 | awk '{print $2}')
echo "Task ID: $TASK_ID"

echo "=== Creating graph ==="
GRAPH_OUTPUT=$($HIVEMIND graph create test-fresh "test-graph" --from-tasks "$TASK_ID" 2>&1)
echo "$GRAPH_OUTPUT"
GRAPH_ID=$(echo "$GRAPH_OUTPUT" | grep "Graph ID:" | awk '{print $3}')
echo "Graph ID: $GRAPH_ID"

echo "=== Creating flow ==="
FLOW_OUTPUT=$($HIVEMIND flow create "$GRAPH_ID" 2>&1)
echo "$FLOW_OUTPUT"
FLOW_ID=$(echo "$FLOW_OUTPUT" | grep "Flow ID:" | awk '{print $3}')
echo "Flow ID: $FLOW_ID"

echo "=== Checking worktrees before start ==="
$HIVEMIND worktree list "$FLOW_ID" 2>&1

echo "=== Starting flow ==="
$HIVEMIND flow start "$FLOW_ID" 2>&1

echo "=== Checking worktrees after start ==="
$HIVEMIND worktree list "$FLOW_ID" 2>&1

echo "=== Listing worktree directory ==="
ls -la "$PWD/.hivemind/worktrees/" 2>&1 || echo "Directory does not exist"

echo "=== Test complete ==="
