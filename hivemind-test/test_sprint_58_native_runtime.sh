#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOST_HOME="${HOME:-}"
DEFAULT_HIVEMIND="$REPO_ROOT/target/debug/hivemind"
if [[ ! -x "$DEFAULT_HIVEMIND" && -n "$HOST_HOME" && -x "$HOST_HOME/.cargo/shared-target/debug/hivemind" ]]; then
  DEFAULT_HIVEMIND="$HOST_HOME/.cargo/shared-target/debug/hivemind"
fi
HIVEMIND="${HIVEMIND:-$DEFAULT_HIVEMIND}"
if [[ -f "$REPO_ROOT/hivemind-test/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "$REPO_ROOT/hivemind-test/.env"
  set +a
fi

MODEL_ID="${OPENROUTER_MODEL_ID:-nvidia/nemotron-3-nano-30b-a3b:free}"
TOKEN_BUDGET="${HIVEMIND_NATIVE_TOKEN_BUDGET:-96000}"
PROMPT_HEADROOM="${HIVEMIND_NATIVE_PROMPT_HEADROOM:-512}"
if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required (set in shell or hivemind-test/.env)." >&2
  exit 1
fi

REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F)"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-native-runtime.log"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-native-runtime-report.md"
FLOW1_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow1-events.json"
FLOW2_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow2-events.json"
FLOW3_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow3-events.json"
FLOW1_NATIVE_SUMMARY_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow1-native-summary.json"
FLOW2_NATIVE_SUMMARY_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow2-native-summary.json"
FLOW3_NATIVE_SUMMARY_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-flow3-native-summary.json"
ATTEMPT_CONTEXT_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-attempt-context.json"
ANALYSIS_PATH="$REPORT_DIR/${DATE_TAG}-sprint58-analysis.json"

mkdir -p "$REPORT_DIR"
exec > >(tee "$LOG_PATH") 2>&1

run() {
  echo
  echo "+ $*"
  "$@"
}

run_sensitive() {
  echo
  echo "+ [sensitive command]"
  "$@"
}

run_capture() {
  local out
  echo >&2
  echo "+ $*" >&2
  out="$("$@" 2>&1)"
  echo "$out" >&2
  printf "%s" "$out"
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    echo "ASSERTION FAILED [$label]: missing '$needle'" >&2
    exit 1
  fi
}

assert_file() {
  [[ -f "$1" ]] || {
    echo "ASSERTION FAILED [file]: missing '$1'" >&2
    exit 1
  }
}

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  grep -Fq -- "$needle" "$path" || {
    echo "ASSERTION FAILED [$label]: '$needle' missing from $path" >&2
    exit 1
  }
}

hm_id() {
  python3 -c 'import json,sys
obj=json.load(sys.stdin)
if isinstance(obj,dict) and "data" in obj and isinstance(obj["data"],dict):
    obj=obj["data"]
for key in ("id","task_id","graph_id","flow_id","attempt_id","project_id"):
    if isinstance(obj,dict) and key in obj:
        print(obj[key]); raise SystemExit(0)
raise SystemExit("no known id field found")'
}

mk_write_directive() {
  local path="$1"
  local content="$2"
  python3 - "$path" "$content" <<'PY'
import json, sys
print("ACT:tool:write_file:" + json.dumps({"path": sys.argv[1], "content": sys.argv[2], "append": False}, separators=(",", ":")))
PY
}

build_prompt_from_directives() {
  local intro="$1"
  shift
  local out="$intro"$'\n'"Emit exactly one directive per turn and follow the numbered steps in order."
  local step=1
  local directive
  for directive in "$@"; do
    out+=$'\n'"$step) $directive"
    step=$((step + 1))
  done
  printf "%s" "$out"
}

redact_secret() {
  python3 -c 'import os,sys
secret=os.environ.get("OPENROUTER_API_KEY","")
payload=sys.stdin.read()
if secret:
    payload=payload.replace(secret,"[REDACTED_OPENROUTER_API_KEY]")
sys.stdout.write(payload)'
}

task_artifacts_ready() {
  local task_id="$1"
  local worktree_path
  worktree_path="$($HIVEMIND -f json worktree inspect "$task_id" | jq -r '.data.path // empty')"
  [[ -n "$worktree_path" && -d "$worktree_path" ]] || return 1

  if [[ -n "${EXECUTOR_TASK_ID:-}" && "$task_id" == "$EXECUTOR_TASK_ID" ]]; then
    [[ -f "$worktree_path/docs/verification.md" ]] || return 1
    grep -Fq -- "$PROOF_PHRASE" "$worktree_path/docs/verification.md" || return 1
    grep -Fq -- "Runtime mode: task_executor" "$worktree_path/docs/verification.md" || return 1
    (cd "$worktree_path" && python3 -m unittest discover -s tests -v >/dev/null 2>&1)
    return $?
  fi

  if [[ -n "${FREEFLOW_TASK_ID:-}" && "$task_id" == "$FREEFLOW_TASK_ID" ]]; then
    [[ -f "$worktree_path/docs/freeflow.md" ]] || return 1
    grep -Fq -- "$PROOF_PHRASE" "$worktree_path/docs/freeflow.md" || return 1
    grep -Fq -- "Mode: freeflow" "$worktree_path/docs/freeflow.md" || return 1
    return 0
  fi

  if [[ -n "${PLANNER_TASK_ID:-}" && "$task_id" == "$PLANNER_TASK_ID" ]]; then
    [[ ! -f "$worktree_path/docs/planner-blocked.md" ]]
    return $?
  fi

  return 0
}

task_has_tool_coverage() {
  local flow_id="$1"
  local task_id="$2"
  shift 2
  local observed
  observed="$($HIVEMIND -f json events stream --flow "$flow_id" --limit 5000 | jq -r --arg task_id "$task_id" '.data[] | .payload | select(.type=="tool_call_completed") | select(((.task_id // .native_correlation.task_id) // "") == $task_id) | .tool_name' | sort -u | tr '\n' ' ')"
  local required
  for required in "$@"; do
    [[ "$observed" == *"$required"* ]] || {
      echo "Task $task_id missing required tool '$required' (observed: $observed)" >&2
      return 1
    }
  done
  return 0
}

complete_checkpoint_if_needed() {
  local flow_id="$1"
  local tick_output="$2"
  [[ "$tick_output" == *"checkpoints_incomplete"* ]] || return 0

  local attempt_id
  attempt_id="$(python3 - "$tick_output" <<'PY'
import re, sys
match = re.search(r"attempt-id ([0-9a-f-]{36})", sys.argv[1])
print(match.group(1) if match else "")
PY
)"
  if [[ -z "$attempt_id" ]]; then
    attempt_id="$($HIVEMIND -f json attempt list --flow "$flow_id" --limit 1 | jq -r '.data[0].attempt_id // empty')"
  fi
  [[ -n "$attempt_id" ]] || {
    echo "Unable to determine attempt_id for checkpoint completion" >&2
    exit 1
  }

  local running_task
  running_task="$($HIVEMIND -f json flow status "$flow_id" | jq -r '[.data.task_executions | to_entries[]? | select(.value.state=="running") | .key][0] // empty')"
  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "sprint58 checkpoint"

  if [[ -n "$running_task" ]]; then
    if [[ -n "${EXECUTOR_TASK_ID:-}" && "$running_task" == "$EXECUTOR_TASK_ID" ]]; then
      local ok=1
      local _try
      for _try in 1 2 3 4 5; do
        if task_artifacts_ready "$running_task" && task_has_tool_coverage "$flow_id" "$running_task" list_files read_file write_file run_command git_status git_diff graph_query; then
          ok=0
          break
        fi
        sleep 1
      done
      if (( ok != 0 )); then
        echo "warning: executor artifacts/tool coverage not fully visible at checkpoint; deferring hard assertions to post-run analysis"
      fi
      run "$HIVEMIND" task complete "$running_task" || true
      return 0
    fi
    if [[ -n "${FREEFLOW_TASK_ID:-}" && "$running_task" == "$FREEFLOW_TASK_ID" ]]; then
      local ok=1
      local _try
      for _try in 1 2 3 4 5; do
        if task_artifacts_ready "$running_task" && task_has_tool_coverage "$flow_id" "$running_task" read_file write_file; then
          ok=0
          break
        fi
        sleep 1
      done
      if (( ok != 0 )); then
        echo "warning: freeflow artifacts/tool coverage not fully visible at checkpoint; deferring hard assertions to post-run analysis"
      fi
      run "$HIVEMIND" task complete "$running_task" || true
      return 0
    fi
    if task_artifacts_ready "$running_task"; then
      run "$HIVEMIND" task complete "$running_task" || true
    else
      run "$HIVEMIND" task abort "$running_task" --reason "missing expected artifacts after checkpoint" || true
    fi
  fi
}

retry_failed_tasks_if_any() {
  local flow_id="$1"
  local failed_tasks
  failed_tasks="$($HIVEMIND -f json flow status "$flow_id" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="failed") | .key')"
  [[ -n "$failed_tasks" ]] || return 0
  while IFS= read -r task_id; do
    [[ -n "$task_id" ]] || continue
    echo
    echo "+ $HIVEMIND task retry $task_id --mode clean"
    local retry_out
    local retry_rc=0
    set +e
    retry_out="$($HIVEMIND task retry "$task_id" --mode clean 2>&1)"
    retry_rc=$?
    set -e
    echo "$retry_out"
    if (( retry_rc != 0 )) && [[ "$retry_out" == *"retry_limit_exceeded"* ]]; then
      "$HIVEMIND" flow abort "$flow_id" || true
      return 2
    fi
  done <<< "$failed_tasks"
}

run_flow_until_terminal() {
  local flow_id="$1"
  local max_ticks="${2:-100}"
  local tick=0
  while (( tick < max_ticks )); do
    tick=$((tick + 1))
    local flow_json state
    flow_json="$($HIVEMIND -f json flow status "$flow_id")"
    state="$(printf '%s' "$flow_json" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
    echo "flow=$flow_id tick=$tick state=$state"
    if [[ "$state" == "completed" || "$state" == "merged" || "$state" == "aborted" ]]; then
      return 0
    fi
    local retry_rc=0
    set +e
    retry_failed_tasks_if_any "$flow_id"
    retry_rc=$?
    set -e
    (( retry_rc == 2 )) && return 1
    local tick_out tick_rc=0
    set +e
    tick_out="$($HIVEMIND flow tick "$flow_id" 2>&1)"
    tick_rc=$?
    set -e
    echo "$tick_out"
    complete_checkpoint_if_needed "$flow_id" "$tick_out"
    set +e
    retry_failed_tasks_if_any "$flow_id"
    retry_rc=$?
    set -e
    (( retry_rc == 2 )) && return 1
    (( tick_rc != 0 )) && sleep 1
  done
  echo "Flow $flow_id did not reach terminal state within $max_ticks ticks" >&2
  return 1
}

run_failure_flow_until_evidence() {
  local flow_id="$1"
  local max_ticks="${2:-12}"
  local tick=0
  local retries=0
  while (( tick < max_ticks )); do
    tick=$((tick + 1))
    echo "failure-flow=$flow_id tick=$tick"
    set +e
    local tick_out
    tick_out="$($HIVEMIND flow tick "$flow_id" 2>&1)"
    local tick_rc=$?
    set -e
    echo "$tick_out"
    local events
    events="$($HIVEMIND -f json events stream --flow "$flow_id" --limit 1000)"
    if printf '%s' "$events" | jq -e '.data[] | .payload | select(.type=="tool_call_failed" and .code=="native_tool_mode_denied")' >/dev/null; then
      return 0
    fi
    local failed_task
    failed_task="$($HIVEMIND -f json flow status "$flow_id" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="failed") | .key' | head -n1)"
    if [[ -n "$failed_task" && $retries -lt 2 ]]; then
      retries=$((retries + 1))
      run "$HIVEMIND" task retry "$failed_task" --mode clean || true
    fi
    (( tick_rc != 0 )) && sleep 1
  done
  echo "Failed to observe planner denial evidence for flow $flow_id" >&2
  return 1
}

runtime_set_mode() {
  local mode="$1"
  mkdir -p "$TMP_BASE/native-state-$mode"
  local cmd=(
    "$HIVEMIND" project runtime-set "$PROJECT_NAME"
    --role worker
    --adapter native
    --binary-path builtin-native
    --model "$MODEL_ID"
    --arg=--max-turns=18
    --arg=--token-budget=$TOKEN_BUDGET
    --timeout-ms 180000
    --max-parallel-tasks 1
  )
  local envs=(
    "HIVEMIND_RUNTIME_ENV_INHERIT=none"
    "HIVEMIND_NATIVE_PROVIDER=openrouter"
    "HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true"
    "HIVEMIND_NATIVE_SANDBOX_MODE=workspace-write"
    "HIVEMIND_NATIVE_APPROVAL_MODE=never"
    "HIVEMIND_NATIVE_NETWORK_PROXY_MODE=off"
    "HIVEMIND_NATIVE_NETWORK_MODE=full"
    "HIVEMIND_NATIVE_AGENT_MODE=$mode"
    "HIVEMIND_NATIVE_TOKEN_BUDGET=$TOKEN_BUDGET"
    "HIVEMIND_NATIVE_PROMPT_HEADROOM=$PROMPT_HEADROOM"
    "HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST=python3,python,git"
    "HIVEMIND_NATIVE_STATE_DIR=$TMP_BASE/native-state-$mode"
    "HIVEMIND_NATIVE_SECRETS_MASTER_KEY=$TEST_MASTER_KEY"
    "PATH=$PATH"
    "OPENROUTER_API_KEY=$OPENROUTER_API_KEY"
  )
  local env_pair
  for env_pair in "${envs[@]}"; do
    cmd+=(--env "$env_pair")
  done
  run_sensitive "${cmd[@]}"
}

echo "=== Sprint 58 Native Runtime Manual Validation ==="
echo "Using hivemind binary: $HIVEMIND"
echo "Using OpenRouter model: $MODEL_ID"

require_cmd "$HIVEMIND"
require_cmd python3
require_cmd jq
require_cmd git
require_cmd sqlite3

TMP_BASE="/tmp/hm-sprint58-native-runtime"
TEST_MASTER_KEY="$(printf 'sprint58-test-master-key' | sha256sum | cut -d' ' -f1)"
export HOME="$TMP_BASE/home"
export HIVEMIND_DATA_DIR="$HOME/.hivemind"
export PYTHONDONTWRITEBYTECODE=1
rm -rf "$TMP_BASE"
mkdir -p "$HOME" "$TMP_BASE/repo"
cd "$TMP_BASE/repo"

PROOF_PHRASE="sprint58-proof-$(date +%s)"

run git init
run git branch -m main
cat > README.md <<EOF
# Sprint 58 Native Runtime Validation

Runtime proof phrase: $PROOF_PHRASE

This repository is used to validate task_executor, freeflow, and planner native runtime modes.
EOF
run git add README.md
run git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

PROJECT_NAME="sprint58-native-runtime"
run "$HIVEMIND" project create "$PROJECT_NAME" --description "Sprint 58 native runtime validation project"
run "$HIVEMIND" project attach-repo "$PROJECT_NAME" "$PWD" --name main --access rw

echo "=== Governance/bootstrap setup ==="
run "$HIVEMIND" project governance init "$PROJECT_NAME"
run "$HIVEMIND" project governance document create "$PROJECT_NAME" architecture-notes --title "Architecture Notes" --owner "qa" --content "Layered API: models, store, service, router, server, docs, tests."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" api-contract --title "API Contract" --owner "qa" --content "GET /health, GET /items, POST /items."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" testing-policy --title "Testing Policy" --owner "qa" --content "Run python3 -m unittest discover -s tests -v before DONE."
run "$HIVEMIND" global system-prompt create sprint58-native-system --content "Emit exactly one THINK/ACT/DONE directive per turn. Follow task steps in order. Never emit DONE until every requested tool action has succeeded."
run "$HIVEMIND" global skill create sprint58-python-skill --name "Python API Builder" --content "Prefer explicit modules, stable function names, and small files."
run "$HIVEMIND" global skill create sprint58-validation-skill --name "Validation Discipline" --content "Use read_file results when asked to quote repository content exactly."
run "$HIVEMIND" global template create sprint58-native-template --system-prompt-id sprint58-native-system --skill-id sprint58-python-skill --skill-id sprint58-validation-skill --document-id architecture-notes --document-id api-contract --document-id testing-policy
run "$HIVEMIND" global template instantiate "$PROJECT_NAME" sprint58-native-template
run "$HIVEMIND" -f json project governance diagnose "$PROJECT_NAME"

INIT_PY='"""hivemind_api package."""'
MODELS_PY=$'"""Domain models for the validation API."""\n\nfrom dataclasses import dataclass\n\n\n@dataclass\nclass Item:\n    name: str\n    quantity: int = 1\n'
STORE_PY=$'"""In-memory store."""\n\nfrom __future__ import annotations\n\nfrom typing import Dict, List\n\nfrom .models import Item\n\n_ITEMS: Dict[str, Item] = {"alpha": Item(name="alpha", quantity=1), "beta": Item(name="beta", quantity=2)}\n\n\ndef list_items() -> List[Item]:\n    return [Item(name=item.name, quantity=item.quantity) for item in _ITEMS.values()]\n\n\ndef upsert_item(name: str, quantity: int = 1) -> Item:\n    existing = _ITEMS.get(name)\n    if existing is None:\n        item = Item(name=name, quantity=max(quantity, 1))\n        _ITEMS[name] = item\n        return Item(name=item.name, quantity=item.quantity)\n    existing.quantity += max(quantity, 1)\n    return Item(name=existing.name, quantity=existing.quantity)\n'
SERVICE_PY=$'"""Service layer."""\n\nfrom __future__ import annotations\n\nfrom .store import list_items, upsert_item\n\n\ndef get_items_payload() -> dict:\n    items = [{"name": item.name, "quantity": item.quantity} for item in list_items()]\n    return {"items": items, "count": len(items)}\n\n\ndef add_item_payload(name: str, quantity: int = 1) -> dict:\n    clean = str(name).strip()\n    if not clean:\n        raise ValueError("name is required")\n    item = upsert_item(clean, quantity)\n    return {"name": item.name, "quantity": item.quantity}\n'
ROUTER_PY=$'"""HTTP route dispatcher."""\n\nfrom __future__ import annotations\n\nimport json\nfrom typing import Dict, Tuple\n\nfrom .service import add_item_payload, get_items_payload\n\nResponse = Tuple[int, Dict[str, object]]\n\n\ndef route_request(method: str, path: str, body: str) -> Response:\n    if method == "GET" and path == "/health":\n        return 200, {"status": "ok"}\n    if method == "GET" and path == "/items":\n        return 200, get_items_payload()\n    if method == "POST" and path == "/items":\n        payload = json.loads(body or "{}")\n        try:\n            item = add_item_payload(payload.get("name", ""), int(payload.get("quantity", 1)))\n        except ValueError as error:\n            return 400, {"error": str(error)}\n        return 201, {"item": item}\n    return 404, {"error": "not found"}\n'
SERVER_PY=$'"""HTTP server wiring."""\n\nfrom __future__ import annotations\n\nimport json\nfrom http.server import BaseHTTPRequestHandler, HTTPServer\n\nfrom .router import route_request\n\n\nclass ApiHandler(BaseHTTPRequestHandler):\n    def _send(self, status: int, payload: dict) -> None:\n        data = json.dumps(payload).encode("utf-8")\n        self.send_response(status)\n        self.send_header("Content-Type", "application/json")\n        self.send_header("Content-Length", str(len(data)))\n        self.end_headers()\n        self.wfile.write(data)\n\n    def do_GET(self) -> None:\n        status, payload = route_request("GET", self.path, "")\n        self._send(status, payload)\n\n    def do_POST(self) -> None:\n        length = int(self.headers.get("Content-Length", "0"))\n        raw = self.rfile.read(length).decode("utf-8") if length else ""\n        status, payload = route_request("POST", self.path, raw)\n        self._send(status, payload)\n\n\ndef build_server(host: str = "127.0.0.1", port: int = 8058) -> HTTPServer:\n    return HTTPServer((host, port), ApiHandler)\n'
MAIN_PY=$'"""Application entrypoint."""\n\nfrom hivemind_api.server import build_server\n\n\nif __name__ == "__main__":\n    server = build_server()\n    print("Serving on http://127.0.0.1:8058")\n    server.serve_forever()\n'
TEST_SERVICE_PY=$'import unittest\n\nfrom hivemind_api.service import add_item_payload, get_items_payload\n\n\nclass ServiceTests(unittest.TestCase):\n    def test_count(self):\n        payload = get_items_payload()\n        self.assertGreaterEqual(payload["count"], 2)\n\n    def test_add_item(self):\n        created = add_item_payload("gamma", 3)\n        self.assertEqual(created["name"], "gamma")\n        self.assertEqual(created["quantity"], 3)\n\n    def test_blank_name(self):\n        with self.assertRaises(ValueError):\n            add_item_payload("   ", 1)\n\n\nif __name__ == "__main__":\n    unittest.main()\n'
TEST_ROUTER_PY=$'import json\nimport unittest\n\nfrom hivemind_api.router import route_request\n\n\nclass RouterTests(unittest.TestCase):\n    def test_health(self):\n        status, payload = route_request("GET", "/health", "")\n        self.assertEqual(status, 200)\n        self.assertEqual(payload["status"], "ok")\n\n    def test_items_round_trip(self):\n        status, payload = route_request("POST", "/items", json.dumps({"name": "omega", "quantity": 4}))\n        self.assertEqual(status, 201)\n        status, payload = route_request("GET", "/items", "")\n        self.assertEqual(status, 200)\n        self.assertGreaterEqual(payload["count"], 3)\n\n\nif __name__ == "__main__":\n    unittest.main()\n'

mkdir -p hivemind_api tests
printf '%s\n' "$INIT_PY" > hivemind_api/__init__.py
printf '%s\n' "$MODELS_PY" > hivemind_api/models.py
printf '%s\n' "$STORE_PY" > hivemind_api/store.py
printf '%s\n' "$SERVICE_PY" > hivemind_api/service.py
printf '%s\n' "$ROUTER_PY" > hivemind_api/router.py
printf '%s\n' "$SERVER_PY" > hivemind_api/server.py
printf '%s\n' "$MAIN_PY" > main.py
printf '%s\n' "$TEST_SERVICE_PY" > tests/test_service.py
printf '%s\n' "$TEST_ROUTER_PY" > tests/test_router.py
run git add README.md hivemind_api main.py tests
run git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Seed validation API"
run "$HIVEMIND" graph snapshot refresh "$PROJECT_NAME"

EXECUTOR_PROMPT=$(cat <<EOF
Inspect and validate the seeded API repository. Emit exactly one directive per turn and follow the steps in order.
1) ACT:tool:list_files:{"path":".","recursive":false}
2) ACT:tool:read_file:{"path":"README.md"}
3) The next directive MUST be a write_file call that creates docs/verification.md. The file must contain exactly these sections:
   - a title line '# Verification Notes'
   - a bullet line '- Proof phrase: <the exact runtime proof phrase from README.md>'
   - a bullet line '- Runtime mode: task_executor'
   - a bullet line '- Validation command: python3 -m unittest discover -s tests -v'
4) ACT:tool:run_command:{"command":"python3","args":["-m","unittest","discover","-s","tests","-v"]}
5) ACT:tool:git_status:{}
6) ACT:tool:git_diff:{"staged":false}
7) ACT:tool:graph_query:{"kind":"filter","path_prefix":"hivemind_api","max_results":25}
8) DONE:verification artifact created and validation completed
EOF
)
FREEFLOW_PROMPT=$(cat <<EOF
Create a freeflow follow-up note. Emit exactly one directive per turn and follow the steps in order.
1) ACT:tool:read_file:{"path":"docs/verification.md"}
2) The next directive MUST be a write_file call that creates docs/freeflow.md. The file must contain:
   - a title line '# Freeflow Notes'
   - a bullet line '- Proof phrase: <the exact proof phrase from docs/verification.md>'
   - a bullet line '- Mode: freeflow'
3) DONE:freeflow note created
EOF
)
PLANNER_PROMPT=$(cat <<EOF
Planner mode denial check.
Your FIRST response must be exactly this directive and nothing else:
ACT:tool:write_file:{"path":"docs/planner-blocked.md","content":"planner mode should not write this file\n","append":false}
Do not output THINK first. Attempt the forbidden write so policy enforcement can reject it.
If you receive another turn after the tool attempt, respond with DONE:planner denial observed.
EOF
)

echo "=== Task executor scenario ==="
runtime_set_mode task_executor
EXECUTOR_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Validate seeded API" --description "$EXECUTOR_PROMPT")"
EXECUTOR_TASK_ID="$(printf '%s' "$EXECUTOR_JSON" | hm_id)"
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$EXECUTOR_TASK_ID" architecture-notes
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$EXECUTOR_TASK_ID" api-contract
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$EXECUTOR_TASK_ID" testing-policy

GRAPH1_JSON="$(run_capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" sprint58-graph-1 --from-tasks "$EXECUTOR_TASK_ID")"
GRAPH1_ID="$(printf '%s' "$GRAPH1_JSON" | hm_id)"
run "$HIVEMIND" graph validate "$GRAPH1_ID"
FLOW1_JSON="$(run_capture "$HIVEMIND" -f json flow create "$GRAPH1_ID" --name sprint58-task-executor)"
FLOW1_ID="$(printf '%s' "$FLOW1_JSON" | hm_id)"
run "$HIVEMIND" flow start "$FLOW1_ID"
run_flow_until_terminal "$FLOW1_ID" 120
FLOW1_STATUS="$($HIVEMIND -f json flow status "$FLOW1_ID" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
[[ "$FLOW1_STATUS" == "completed" ]] || { echo "Flow1 not completed: $FLOW1_STATUS" >&2; exit 1; }
run "$HIVEMIND" merge prepare "$FLOW1_ID"
run "$HIVEMIND" merge approve "$FLOW1_ID"
run "$HIVEMIND" merge execute "$FLOW1_ID" --mode local
run python3 -m unittest discover -s tests -v
find . -type d -name '__pycache__' -prune -exec rm -rf {} +
run git reset --hard HEAD
run git clean -fd
assert_file docs/verification.md
assert_file_contains docs/verification.md "$PROOF_PHRASE" "task-executor proof phrase"

echo "=== Freeflow scenario ==="
runtime_set_mode freeflow
FREEFLOW_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Create freeflow note" --description "$FREEFLOW_PROMPT")"
FREEFLOW_TASK_ID="$(printf '%s' "$FREEFLOW_JSON" | hm_id)"
GRAPH2_JSON="$(run_capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" sprint58-graph-2 --from-tasks "$FREEFLOW_TASK_ID")"
GRAPH2_ID="$(printf '%s' "$GRAPH2_JSON" | hm_id)"
FLOW2_JSON="$(run_capture "$HIVEMIND" -f json flow create "$GRAPH2_ID" --name sprint58-freeflow)"
FLOW2_ID="$(printf '%s' "$FLOW2_JSON" | hm_id)"
run "$HIVEMIND" flow start "$FLOW2_ID"
run_flow_until_terminal "$FLOW2_ID" 60
FLOW2_STATUS="$($HIVEMIND -f json flow status "$FLOW2_ID" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
[[ "$FLOW2_STATUS" == "completed" ]] || { echo "Flow2 not completed: $FLOW2_STATUS" >&2; exit 1; }
run "$HIVEMIND" merge prepare "$FLOW2_ID"
run "$HIVEMIND" merge approve "$FLOW2_ID"
run git reset --hard HEAD
run git clean -fd
run "$HIVEMIND" merge execute "$FLOW2_ID" --mode local

echo "=== Planner denial scenario ==="
runtime_set_mode planner
PLANNER_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Planner denial check" --description "$PLANNER_PROMPT")"
PLANNER_TASK_ID="$(printf '%s' "$PLANNER_JSON" | hm_id)"
GRAPH3_JSON="$(run_capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" sprint58-graph-3 --from-tasks "$PLANNER_TASK_ID")"
GRAPH3_ID="$(printf '%s' "$GRAPH3_JSON" | hm_id)"
FLOW3_JSON="$(run_capture "$HIVEMIND" -f json flow create "$GRAPH3_ID" --name sprint58-planner)"
FLOW3_ID="$(printf '%s' "$FLOW3_JSON" | hm_id)"
run "$HIVEMIND" flow start "$FLOW3_ID"
run_failure_flow_until_evidence "$FLOW3_ID" 12

echo "=== Event capture ==="
FLOW1_EVENTS="$(printf '%s' "$($HIVEMIND -f json events stream --flow "$FLOW1_ID" --limit 6000)" | redact_secret)"
FLOW2_EVENTS="$(printf '%s' "$($HIVEMIND -f json events stream --flow "$FLOW2_ID" --limit 2500)" | redact_secret)"
FLOW3_EVENTS="$(printf '%s' "$($HIVEMIND -f json events stream --flow "$FLOW3_ID" --limit 2500)" | redact_secret)"
FLOW1_NATIVE_SUMMARY="$(printf '%s' "$($HIVEMIND -f json events native-summary --flow "$FLOW1_ID" --limit 6000 --verify)" | redact_secret)"
FLOW2_NATIVE_SUMMARY="$(printf '%s' "$($HIVEMIND -f json events native-summary --flow "$FLOW2_ID" --limit 2500 --verify)" | redact_secret)"
FLOW3_NATIVE_SUMMARY="$(printf '%s' "$($HIVEMIND -f json events native-summary --flow "$FLOW3_ID" --limit 2500 --verify)" | redact_secret)"
printf '%s\n' "$FLOW1_EVENTS" > "$FLOW1_EVENTS_PATH"
printf '%s\n' "$FLOW2_EVENTS" > "$FLOW2_EVENTS_PATH"
printf '%s\n' "$FLOW3_EVENTS" > "$FLOW3_EVENTS_PATH"
printf '%s\n' "$FLOW1_NATIVE_SUMMARY" > "$FLOW1_NATIVE_SUMMARY_PATH"
printf '%s\n' "$FLOW2_NATIVE_SUMMARY" > "$FLOW2_NATIVE_SUMMARY_PATH"
printf '%s\n' "$FLOW3_NATIVE_SUMMARY" > "$FLOW3_NATIVE_SUMMARY_PATH"

FLOW1_ATTEMPTS="$($HIVEMIND -f json attempt list --flow "$FLOW1_ID" --limit 30)"
FLOW1_CONTEXT_FOUND=0
for attempt_id in $(printf '%s' "$FLOW1_ATTEMPTS" | jq -r '.data[].attempt_id'); do
  [[ -n "$attempt_id" ]] || continue
  CTX_JSON="$(printf '%s' "$($HIVEMIND -f json attempt inspect "$attempt_id" --context)" | redact_secret)"
  if printf '%s' "$CTX_JSON" | jq -e '.context.manifest_hash and .context.inputs_hash and .context.rendered_prompt_hash and .context.delivered_context_hash' >/dev/null; then
    printf '%s\n' "$CTX_JSON" > "$ATTEMPT_CONTEXT_PATH"
    FLOW1_CONTEXT_FOUND=1
    break
  fi
done
(( FLOW1_CONTEXT_FOUND == 1 )) || { echo "No flow1 attempt exposed full context hashes" >&2; exit 1; }

python3 - \
  "$FLOW1_EVENTS_PATH" "$FLOW2_EVENTS_PATH" "$FLOW3_EVENTS_PATH" \
  "$FLOW1_NATIVE_SUMMARY_PATH" "$FLOW2_NATIVE_SUMMARY_PATH" "$FLOW3_NATIVE_SUMMARY_PATH" \
  "$ANALYSIS_PATH" "$PROMPT_HEADROOM" <<'PY'
import json, sys
flow1_path, flow2_path, flow3_path, flow1_summary_path, flow2_summary_path, flow3_summary_path, out_path, prompt_headroom = sys.argv[1:9]
prompt_headroom = int(prompt_headroom)

def load(path):
    with open(path, 'r', encoding='utf-8') as fh:
        return json.load(fh).get('data', [])

def load_obj(path):
    with open(path, 'r', encoding='utf-8') as fh:
        return json.load(fh).get('data', {})

def payloads(events, typ):
    return [event.get('payload', {}) for event in events if event.get('payload', {}).get('type') == typ]

def ordered_call_ids(events):
    seen = {}
    for idx, event in enumerate(events):
        payload = event.get('payload', {})
        typ = payload.get('type')
        if typ not in {'tool_call_requested', 'tool_call_started', 'tool_call_completed', 'tool_call_failed'}:
            continue
        entry = seen.setdefault(payload.get('call_id'), {})
        entry[typ] = idx
    return seen

flow1 = load(flow1_path)
flow2 = load(flow2_path)
flow3 = load(flow3_path)
flow1_summary = load_obj(flow1_summary_path)
flow2_summary = load_obj(flow2_summary_path)
flow3_summary = load_obj(flow3_summary_path)

flow1_inv = payloads(flow1, 'agent_invocation_started')
assert flow1_inv and flow1_inv[0].get('agent_mode') == 'task_executor'
assert 'write_file' in flow1_inv[0].get('allowed_tools', [])
assert 'run_command' in flow1_inv[0].get('allowed_tools', [])

flow1_turns = payloads(flow1, 'agent_turn_started')
assert flow1_turns and any(t.get('prompt_hash') for t in flow1_turns)
assert any(t.get('context_manifest_hash') for t in flow1_turns)
assert any(t.get('delivered_context_hash') for t in flow1_turns)

flow1_requests = payloads(flow1, 'model_request_prepared')
assert flow1_requests and any(r.get('prompt_headroom') == prompt_headroom for r in flow1_requests)
assert any(int(r.get('visible_item_count', 0)) > 0 for r in flow1_requests)
assert any(r.get('mode_contract_hash') for r in flow1_turns)
assert any(r.get('inputs_hash') for r in flow1_turns)
assert any(int(r.get('rendered_prompt_bytes', 0)) > 0 for r in flow1_requests)
assert any(int(r.get('tool_contract_count', 0)) > 0 for r in flow1_requests)

completed_turns = {p['turn_index']: p for p in payloads(flow1, 'agent_turn_completed')}
requests_by_turn = {p['turn_index']: p for p in flow1_requests}
reuse_evidence = False
for turn_index, turn in completed_turns.items():
    next_req = requests_by_turn.get(turn_index + 1)
    cur_req = requests_by_turn.get(turn_index)
    if not next_req or not cur_req:
        continue
    if int(turn.get('tool_result_count', 0)) > 0 and int(next_req.get('visible_item_count', 0)) > int(cur_req.get('visible_item_count', 0)):
        reuse_evidence = True
        break
assert reuse_evidence

flow1_tools = {p.get('tool_name') for p in payloads(flow1, 'tool_call_completed')}
for tool in ['list_files', 'read_file', 'write_file', 'run_command', 'git_status', 'git_diff', 'graph_query']:
    assert tool in flow1_tools

ordering_ok = True
for call_id, marks in ordered_call_ids(flow1).items():
    if 'tool_call_completed' in marks:
        ordering_ok &= marks.get('tool_call_requested', -1) < marks.get('tool_call_started', -1) < marks.get('tool_call_completed', -1)
    if 'tool_call_failed' in marks:
        ordering_ok &= marks.get('tool_call_requested', -1) < marks.get('tool_call_started', -1) < marks.get('tool_call_failed', -1)
assert ordering_ok

flow2_inv = payloads(flow2, 'agent_invocation_started')
assert flow2_inv and flow2_inv[0].get('agent_mode') == 'freeflow'
assert 'write_file' in flow2_inv[0].get('allowed_tools', [])
flow2_tools = {p.get('tool_name') for p in payloads(flow2, 'tool_call_completed')}
assert {'read_file', 'write_file'}.issubset(flow2_tools)

flow3_inv = payloads(flow3, 'agent_invocation_started')
assert flow3_inv and flow3_inv[0].get('agent_mode') == 'planner'
assert 'write_file' not in flow3_inv[0].get('allowed_tools', [])
planner_failures = payloads(flow3, 'tool_call_failed')
assert any(
    p.get('code') == 'native_tool_mode_denied'
    and 'agent_mode:planner' in p.get('policy_tags', [])
    and p.get('policy_source') == 'agent_mode_policy'
    for p in planner_failures
)
planner_done = payloads(flow3, 'agent_invocation_completed')
assert planner_done and planner_done[-1].get('success') is False

for summary in [flow1_summary, flow2_summary, flow3_summary]:
    assert summary.get('verification', {}).get('passed') is True
    assert int(summary.get('invocation_count', 0)) >= 1

flow1_turn_summaries = flow1_summary.get('invocations', [])[0].get('turns', [])
assert flow1_turn_summaries
assert any(turn.get('mode_contract_hash') for turn in flow1_turn_summaries)
assert any(int(turn.get('rendered_prompt_bytes', 0)) > 0 for turn in flow1_turn_summaries)
assert any(int(turn.get('tool_contract_count', 0)) > 0 for turn in flow1_turn_summaries)
assert any(
    int(turn.get('tool_result_items_visible', 0)) > 0
    and turn.get('latest_tool_result_turn_index') is not None
    for turn in flow1_turn_summaries[1:]
)

flow3_denials = flow3_summary.get('invocations', [])[0].get('denied_tools', [])
assert any(denial.get('policy_source') == 'agent_mode_policy' for denial in flow3_denials)

analysis = {
    'flow1_turn_count': len(flow1_turns),
    'flow1_completed_tools': sorted(flow1_tools),
    'flow1_reuse_evidence': reuse_evidence,
    'flow1_event_ordering_ok': ordering_ok,
    'flow1_summary_turns': len(flow1_turn_summaries),
    'flow2_completed_tools': sorted(flow2_tools),
    'flow2_summary_verified': flow2_summary.get('verification', {}).get('passed'),
    'flow3_failure_codes': sorted({p.get('code') for p in planner_failures}),
    'flow3_denied_tools': [d.get('tool_name') for d in flow3_denials],
}
with open(out_path, 'w', encoding='utf-8') as fh:
    json.dump(analysis, fh, indent=2)
print(json.dumps(analysis, indent=2))
PY

assert_file docs/freeflow.md
assert_file_contains docs/freeflow.md "$PROOF_PHRASE" "freeflow proof phrase"

FLOW1_TOOLS="$(printf '%s' "$FLOW1_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
FLOW2_TOOLS="$(printf '%s' "$FLOW2_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
FLOW3_FAILURE_CODES="$(printf '%s' "$FLOW3_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_failed") | .code' | sort -u | tr '\n' ' ')"
FLOW1_EVENT_COUNT="$(printf '%s' "$FLOW1_EVENTS" | jq '.data | length')"
FLOW2_EVENT_COUNT="$(printf '%s' "$FLOW2_EVENTS" | jq '.data | length')"
FLOW3_EVENT_COUNT="$(printf '%s' "$FLOW3_EVENTS" | jq '.data | length')"

cat > "$REPORT_PATH" <<EOF
# Sprint 58 Native Runtime Report ($DATE_TAG)

## Scope
- Real OpenRouter-backed validation for Sprint 58 native tool loop, agent modes, and prompt assembly metadata.
- Task-executor success flow with multi-turn tool use, tool-result reuse, tests, git inspection, and graph query.
- Freeflow attribution flow proving mutating-tool availability.
- Planner denial flow proving mode-aware tool rejection.

## Environment
- Hivemind binary: $HIVEMIND
- Model: $MODEL_ID
- Project: $PROJECT_NAME
- Proof phrase: $PROOF_PHRASE
- Prompt headroom: $PROMPT_HEADROOM

## Flow Results
- Flow 1 (task_executor): $FLOW1_ID / $FLOW1_STATUS
- Flow 2 (freeflow): $FLOW2_ID / $FLOW2_STATUS
- Flow 3 (planner denial): $FLOW3_ID / denial observed

## Assertions Passed
- Task executor invocation exposed agent_mode=task_executor, allowed tools, prompt hashes, context hashes, and prompt headroom.
- Multi-turn in-loop execution was observed with increasing visible_item_count after prior tool results.
- The verification artifact quoted the exact proof phrase obtained from a prior read_file call.
- Freeflow invocation exposed agent_mode=freeflow and successfully completed read_file + write_file.
- Planner invocation exposed agent_mode=planner, omitted write_file from allowed_tools, and emitted native_tool_mode_denied with planner policy tags.

## Metrics
- Flow 1 tools: $FLOW1_TOOLS
- Flow 2 tools: $FLOW2_TOOLS
- Flow 3 failure codes: $FLOW3_FAILURE_CODES
- Flow 1 event count: $FLOW1_EVENT_COUNT
- Flow 2 event count: $FLOW2_EVENT_COUNT
- Flow 3 event count: $FLOW3_EVENT_COUNT

## Artifacts
- Log: $LOG_PATH
- Report: $REPORT_PATH
- Flow 1 events: $FLOW1_EVENTS_PATH
- Flow 2 events: $FLOW2_EVENTS_PATH
- Flow 3 events: $FLOW3_EVENTS_PATH
- Flow 1 native summary: $FLOW1_NATIVE_SUMMARY_PATH
- Flow 2 native summary: $FLOW2_NATIVE_SUMMARY_PATH
- Flow 3 native summary: $FLOW3_NATIVE_SUMMARY_PATH
- Attempt context: $ATTEMPT_CONTEXT_PATH
- Analysis: $ANALYSIS_PATH
EOF

echo
echo "SPRINT58_RESULT=PASS"
echo "REPORT_PATH=$REPORT_PATH"
echo "LOG_PATH=$LOG_PATH"