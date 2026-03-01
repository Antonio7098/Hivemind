#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HIVEMIND="${HIVEMIND:-/home/antonio/.cargo/shared-target/debug/hivemind}"
if [[ -f "$REPO_ROOT/.env" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$REPO_ROOT/.env"
  set +a
fi
MODEL_ID="${OPENROUTER_MODEL_ID:-nvidia/nemotron-3-nano-30b-a3b:free}"
if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
  echo "OPENROUTER_API_KEY is required (set in shell or $REPO_ROOT/.env)." >&2
  exit 1
fi
REPORT_DIR="$REPO_ROOT/hivemind-test/test-report"
DATE_TAG="$(date +%F)"
LOG_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-beta.log"
UNIT_LOG_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-unit.log"
FLOW1_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-flow1-events.json"
FLOW2_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-flow2-events.json"
PROJECT_EVENTS_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-project-events.json"
REPORT_PATH="$REPORT_DIR/${DATE_TAG}-phase45-native-runtime-beta-report.md"

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
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "missing required command: $cmd" >&2
    exit 1
  fi
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
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "ASSERTION FAILED [file]: missing '$path'" >&2
    exit 1
  fi
}

hm_id() {
  python3 -c '
import json
import sys

obj = json.load(sys.stdin)
if isinstance(obj, dict) and "data" in obj and isinstance(obj["data"], dict):
    obj = obj["data"]

for key in ("id", "task_id", "graph_id", "flow_id", "attempt_id", "project_id"):
    if isinstance(obj, dict) and key in obj:
        print(obj[key])
        raise SystemExit(0)

raise SystemExit("no known id field found")
'
}

mk_write_directive() {
  local path="$1"
  local content="$2"
  python3 - "$path" "$content" <<'PY'
import json
import sys

path = sys.argv[1]
content = sys.argv[2]
print(
    "ACT:tool:write_file:"
    + json.dumps(
        {"path": path, "content": content, "append": False},
        separators=(",", ":"),
    )
)
PY
}

build_prompt_from_directives() {
  local intro="$1"
  shift
  local out="$intro"$'\n'"Emit exactly one directive per turn."
  local step=1
  local directive
  for directive in "$@"; do
    out+=$'\n'"$step) $directive"
    step=$((step + 1))
  done
  printf "%s" "$out"
}

redact_secret() {
  python3 -c 'import sys
secret = sys.argv[1]
payload = sys.stdin.read()
if secret:
    payload = payload.replace(secret, "[REDACTED_OPENROUTER_API_KEY]")
sys.stdout.write(payload)
' "$OPENROUTER_API_KEY"
}

task_artifacts_ready() {
  local task_id="$1"
  local worktree_path
  worktree_path="$("$HIVEMIND" -f json worktree inspect "$task_id" | jq -r '.data.path // empty')"
  if [[ -z "$worktree_path" || ! -d "$worktree_path" ]]; then
    return 1
  fi

  if [[ -n "${TASK_A_ID:-}" && "$task_id" == "$TASK_A_ID" ]]; then
    [[ -f "$worktree_path/hivemind_api/models.py" && -f "$worktree_path/hivemind_api/store.py" && -f "$worktree_path/hivemind_api/service.py" ]] || return 1
    (cd "$worktree_path" && python3 -m py_compile hivemind_api/models.py hivemind_api/store.py hivemind_api/service.py >/dev/null 2>&1)
    return $?
  fi

  if [[ -n "${TASK_B_ID:-}" && "$task_id" == "$TASK_B_ID" ]]; then
    [[ -f "$worktree_path/hivemind_api/router.py" && -f "$worktree_path/hivemind_api/server.py" ]] || return 1
    (cd "$worktree_path" && python3 -m py_compile hivemind_api/router.py hivemind_api/server.py >/dev/null 2>&1)
    return $?
  fi

  if [[ -n "${TASK_C_ID:-}" && "$task_id" == "$TASK_C_ID" ]]; then
    [[ -f "$worktree_path/tests/test_service.py" && -f "$worktree_path/tests/test_router.py" && -f "$worktree_path/README.md" && -f "$worktree_path/main.py" ]] || return 1
    (cd "$worktree_path" && python3 -m unittest discover -s tests -v >/dev/null 2>&1)
    return $?
  fi

  if [[ -n "${TASK_D_ID:-}" && "$task_id" == "$TASK_D_ID" ]]; then
    [[ -f "$worktree_path/docs/verification.md" ]]
    return $?
  fi

  return 0
}

task_has_tool_coverage() {
  local flow_id="$1"
  local task_id="$2"
  shift 2
  local observed
  observed="$("$HIVEMIND" -f json events stream --flow "$flow_id" --limit 5000 | jq -r --arg task_id "$task_id" '.data[] | .payload | select(.type=="tool_call_completed") | select(((.task_id // .native_correlation.task_id) // "") == $task_id) | .tool_name' | sort -u | tr '\n' ' ')"
  local required
  for required in "$@"; do
    if [[ "$observed" != *"$required"* ]]; then
      echo "Task $task_id missing required tool '$required' (observed: $observed)"
      return 1
    fi
  done
  return 0
}

complete_checkpoint_if_needed() {
  local flow_id="$1"
  local tick_output="$2"
  if [[ "$tick_output" != *"checkpoints_incomplete"* ]]; then
    return 0
  fi

  local attempt_id
  attempt_id="$(python3 - "$tick_output" <<'PY'
import re
import sys

text = sys.argv[1]
match = re.search(r"attempt-id ([0-9a-f-]{36})", text)
print(match.group(1) if match else "")
PY
)"
  if [[ -z "$attempt_id" ]]; then
    attempt_id="$("$HIVEMIND" -f json attempt list --flow "$flow_id" --limit 1 | jq -r '.data[0].attempt_id // empty')"
  fi
  [[ -n "$attempt_id" ]] || {
    echo "Unable to determine attempt_id for checkpoint completion" >&2
    exit 1
  }

  local running_task
  running_task="$("$HIVEMIND" -f json flow status "$flow_id" | jq -r '[.data.task_executions | to_entries[]? | select(.value.state=="running") | .key][0] // empty')"

  run "$HIVEMIND" checkpoint complete --attempt-id "$attempt_id" --id checkpoint-1 --summary "phase45-native-runtime checkpoint"
  if [[ -n "$running_task" ]]; then
    if [[ -n "${TASK_C_ID:-}" && "$running_task" == "$TASK_C_ID" ]]; then
      if task_artifacts_ready "$running_task" && task_has_tool_coverage "$flow_id" "$running_task" write_file run_command git_status git_diff graph_query; then
        run "$HIVEMIND" task complete "$running_task" || true
      else
        echo "Task $running_task missing required artifacts or tool coverage; aborting to force retry"
        run "$HIVEMIND" task abort "$running_task" --reason "missing required artifacts/tool coverage before completion checkpoint" || true
      fi
      return 0
    fi
    if [[ -n "${TASK_D_ID:-}" && "$running_task" == "$TASK_D_ID" ]]; then
      if task_artifacts_ready "$running_task" && task_has_tool_coverage "$flow_id" "$running_task" run_command write_file; then
        run "$HIVEMIND" task complete "$running_task" || true
      else
        echo "Task $running_task missing required verification artifacts/tool coverage; aborting to force retry"
        run "$HIVEMIND" task abort "$running_task" --reason "missing required verification artifacts/tool coverage before completion checkpoint" || true
      fi
      return 0
    fi
    if task_artifacts_ready "$running_task"; then
      run "$HIVEMIND" task complete "$running_task" || true
    else
      echo "Task $running_task missing expected artifacts after checkpoint; aborting task to force retry"
      run "$HIVEMIND" task abort "$running_task" --reason "missing expected artifacts after checkpoint" || true
    fi
  fi
}

retry_failed_tasks_if_any() {
  local flow_id="$1"
  local failed_tasks
  failed_tasks="$("$HIVEMIND" -f json flow status "$flow_id" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="failed") | .key')"
  if [[ -z "$failed_tasks" ]]; then
    return 0
  fi
  while IFS= read -r task_id; do
    [[ -n "$task_id" ]] || continue
    echo
    echo "+ $HIVEMIND task retry $task_id --mode clean"
    local retry_out
    local retry_rc=0
    set +e
    retry_out="$("$HIVEMIND" task retry "$task_id" --mode clean 2>&1)"
    retry_rc=$?
    set -e
    echo "$retry_out"
    if (( retry_rc != 0 )) && [[ "$retry_out" == *"retry_limit_exceeded"* ]]; then
      echo "Retry limit exceeded for task=$task_id; aborting flow=$flow_id to avoid infinite loop" >&2
      "$HIVEMIND" flow abort "$flow_id" || true
      return 2
    fi
  done <<< "$failed_tasks"
}

run_flow_until_terminal() {
  local flow_id="$1"
  local max_ticks="${2:-80}"
  local tick=0

  while (( tick < max_ticks )); do
    tick=$((tick + 1))
    local flow_json
    flow_json="$("$HIVEMIND" -f json flow status "$flow_id")"
    local state
    state="$(printf '%s' "$flow_json" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
    local running_tasks
    running_tasks="$(printf '%s' "$flow_json" | jq -r '.data.task_executions | to_entries[]? | select(.value.state=="running") | .key' | paste -sd, -)"
    running_tasks="${running_tasks:-none}"
    echo "flow=$flow_id tick=$tick state=$state running=$running_tasks"

    if [[ "$state" == "completed" || "$state" == "merged" || "$state" == "aborted" ]]; then
      return 0
    fi

    local retry_rc=0
    set +e
    retry_failed_tasks_if_any "$flow_id"
    retry_rc=$?
    set -e
    if (( retry_rc == 2 )); then
      return 1
    fi

    local tick_out=""
    local tick_rc=0
    set +e
    tick_out="$("$HIVEMIND" flow tick "$flow_id" 2>&1)"
    tick_rc=$?
    set -e
    echo "$tick_out"

    complete_checkpoint_if_needed "$flow_id" "$tick_out"

    set +e
    retry_failed_tasks_if_any "$flow_id"
    retry_rc=$?
    set -e
    if (( retry_rc == 2 )); then
      return 1
    fi

    if (( tick_rc != 0 )); then
      sleep 1
    fi
  done

  echo "Flow $flow_id did not reach terminal state within $max_ticks ticks" >&2
  return 1
}

run_phase45_unit_matrix() {
  : > "$UNIT_LOG_PATH"
  local filters=(
    "native::startup_hardening::tests::"
    "adapters::runtime::tests::protected_runtime_env_filters_sensitive_and_reserved_inherited_vars"
    "core::registry::tests::tick_flow_emits_runtime_environment_prepared_with_provenance"
    "native::tool_engine::tests::"
    "openrouter_retries_rate_limit_with_backoff_and_telemetry"
    "openrouter_activates_fallback_transport_after_primary_failure"
    "openrouter_classifies_incomplete_terminal_event"
    "openrouter_classifies_idle_timeout"
    "native::runtime_hardening::tests::"
  )
  local ignored_filters=(
    "native::tool_engine::tests::exec_command_and_write_stdin_support_interactive_session"
    "native::tool_engine::tests::exec_command_prunes_sessions_when_cap_exceeded"
    "native::tool_engine::tests::write_stdin_reports_truncation_metadata"
  )

  local filter
  for filter in "${filters[@]}"; do
    echo
    echo "=== cargo test $filter ===" | tee -a "$UNIT_LOG_PATH"
    (
      cd "$REPO_ROOT"
      cargo test "$filter" -- --nocapture
    ) 2>&1 | tee -a "$UNIT_LOG_PATH"
  done

  for filter in "${ignored_filters[@]}"; do
    echo
    echo "=== cargo test $filter (ignored) ===" | tee -a "$UNIT_LOG_PATH"
    (
      cd "$REPO_ROOT"
      cargo test "$filter" -- --ignored --nocapture
    ) 2>&1 | tee -a "$UNIT_LOG_PATH"
  done
}

echo "=== Phase 4.5 Native Runtime Beta ==="
echo "Using hivemind binary: $HIVEMIND"
echo "Using OpenRouter model: $MODEL_ID"

require_cmd "$HIVEMIND"
require_cmd python3
require_cmd jq
require_cmd git
require_cmd cargo
require_cmd sqlite3

echo
echo "=== Unit matrix: Sprint 52-57 feature coverage ==="
run_phase45_unit_matrix

TMP_BASE="/tmp/hm-phase45-native-runtime-beta"
export HOME="$TMP_BASE/home"
export HIVEMIND_DATA_DIR="$HOME/.hivemind"
export BETA_PHASE45_SECRET_TOKEN="phase45-sensitive-token"
export HIVEMIND_RUNTIME_INTERNAL_PHASE45_RESERVED="phase45-reserved"
rm -rf "$TMP_BASE"
mkdir -p "$HOME" "$TMP_BASE/repo" "$TMP_BASE/bin"
cat > "$TMP_BASE/bin/python" <<'EOF'
#!/usr/bin/env bash
exec python3 "$@"
EOF
chmod +x "$TMP_BASE/bin/python"
export PATH="$TMP_BASE/bin:$PATH"
cd "$TMP_BASE/repo"

run git init
run git branch -m main
echo "# Hivemind Native Runtime Beta API" > README.md
run git add README.md
run git -c user.name=Hivemind -c user.email=hivemind@example.com commit -m "Initial commit"

PROJECT_NAME="phase45-native-runtime-beta-api"
run "$HIVEMIND" project create "$PROJECT_NAME" --description "Phase 4.5 native runtime hardening beta API"
run "$HIVEMIND" project attach-repo "$PROJECT_NAME" "$PWD" --name main --access rw

echo "=== Governance/bootstrap setup ==="
run "$HIVEMIND" project governance init "$PROJECT_NAME"
run "$HIVEMIND" project governance document create "$PROJECT_NAME" architecture-notes --title "Architecture Notes" --owner "qa" --content "Layered API: models, store, service, router, server."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" api-contract --title "API Contract" --owner "qa" --content "GET /health, GET /items, POST /items."
run "$HIVEMIND" project governance document create "$PROJECT_NAME" testing-policy --title "Testing Policy" --owner "qa" --content "Run unittest suite before DONE."
run "$HIVEMIND" global system-prompt create phase45-native-system --content "Follow numbered directives exactly. Emit one THINK/ACT/DONE directive per turn. Never call run_command unless the task prompt explicitly includes it. When running Python commands, always use python3."
run "$HIVEMIND" global skill create phase45-python-skill --name "Python API Builder" --content "Keep module boundaries explicit and deterministic."
run "$HIVEMIND" global skill create phase45-testing-skill --name "Test Discipline" --content "Run unit tests and inspect git status only when explicitly requested by the task prompt; use python3, never python."
run "$HIVEMIND" global template create phase45-native-template --system-prompt-id phase45-native-system --skill-id phase45-python-skill --skill-id phase45-testing-skill --document-id architecture-notes --document-id api-contract --document-id testing-policy
run "$HIVEMIND" global template instantiate "$PROJECT_NAME" phase45-native-template

cat > /tmp/hm_phase45_constitution.yaml <<'YAML'
version: 1
schema_version: constitution.v1
compatibility:
  minimum_hivemind_version: 0.1.32
  governance_schema_version: governance.v1
partitions: []
rules: []
YAML

run "$HIVEMIND" constitution init "$PROJECT_NAME" --from-file /tmp/hm_phase45_constitution.yaml --confirm
run "$HIVEMIND" constitution validate "$PROJECT_NAME"
run "$HIVEMIND" constitution check --project "$PROJECT_NAME"
run "$HIVEMIND" graph snapshot refresh "$PROJECT_NAME"
run "$HIVEMIND" -f json project governance diagnose "$PROJECT_NAME"

COMMON_RUNTIME_ENVS=(
  "HIVEMIND_RUNTIME_ENV_INHERIT=all"
  "HIVEMIND_NATIVE_PROVIDER=openrouter"
  "HIVEMIND_NATIVE_CAPTURE_FULL_PAYLOADS=true"
  "HIVEMIND_NATIVE_SANDBOX_MODE=workspace-write"
  "HIVEMIND_NATIVE_APPROVAL_MODE=never"
  "HIVEMIND_NATIVE_NETWORK_PROXY_MODE=off"
  "HIVEMIND_NATIVE_NETWORK_MODE=full"
  "HIVEMIND_NATIVE_TOOL_RUN_COMMAND_ALLOWLIST=python3,python,git"
  "PATH=$TMP_BASE/bin:$PATH"
  "HIVEMIND_NATIVE_EXEC_SESSION_CAP=8"
  "HIVEMIND_NATIVE_STATE_DIR=$TMP_BASE/native-state"
  "HIVEMIND_NATIVE_SECRETS_MASTER_KEY=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  "OPENROUTER_API_KEY=$OPENROUTER_API_KEY"
)

echo "=== Runtime setup (native OpenRouter with hardening env) ==="
PROJECT_RUNTIME_CMD=(
  "$HIVEMIND" project runtime-set "$PROJECT_NAME"
  --role worker
  --adapter native
  --binary-path builtin-native
  --model "$MODEL_ID"
  --arg=--max-turns=16
  --timeout-ms 180000
  --max-parallel-tasks 2
)
for env_pair in "${COMMON_RUNTIME_ENVS[@]}"; do
  PROJECT_RUNTIME_CMD+=(--env "$env_pair")
done
run_sensitive "${PROJECT_RUNTIME_CMD[@]}"

INIT_PY=$(cat <<'EOF'
"""hivemind_api package."""
EOF
)

MODELS_PY=$(cat <<'EOF'
"""Domain models for the beta API."""

from dataclasses import dataclass, field
from typing import Dict


@dataclass
class Item:
    name: str
    quantity: int = 1
    metadata: Dict[str, str] = field(default_factory=dict)
EOF
)

STORE_PY=$(cat <<'EOF'
"""Thread-safe in-memory store."""

from __future__ import annotations

from threading import Lock
from typing import Dict, List

from .models import Item

_LOCK = Lock()
_ITEMS: Dict[str, Item] = {
    "alpha": Item(name="alpha", quantity=1),
    "beta": Item(name="beta", quantity=2),
}


def list_items() -> List[Item]:
    with _LOCK:
        return [Item(name=item.name, quantity=item.quantity, metadata=dict(item.metadata)) for item in _ITEMS.values()]


def upsert_item(name: str, quantity: int = 1) -> Item:
    with _LOCK:
        existing = _ITEMS.get(name)
        if existing is None:
            item = Item(name=name, quantity=max(quantity, 1))
            _ITEMS[name] = item
            return Item(name=item.name, quantity=item.quantity, metadata=dict(item.metadata))
        existing.quantity += max(quantity, 1)
        return Item(name=existing.name, quantity=existing.quantity, metadata=dict(existing.metadata))


def reset_store() -> None:
    with _LOCK:
        _ITEMS.clear()
        _ITEMS["alpha"] = Item(name="alpha", quantity=1)
        _ITEMS["beta"] = Item(name="beta", quantity=2)
EOF
)

SERVICE_PY=$(cat <<'EOF'
"""Service-level API logic."""

from __future__ import annotations

from typing import Dict

from .store import list_items, upsert_item


def get_items_payload() -> Dict[str, object]:
    items = [{"name": item.name, "quantity": item.quantity} for item in list_items()]
    return {"items": items, "count": len(items)}


def add_item_payload(name: str, quantity: int = 1) -> Dict[str, object]:
    clean = str(name).strip()
    if not clean:
        raise ValueError("name is required")
    item = upsert_item(clean, quantity)
    return {"name": item.name, "quantity": item.quantity}
EOF
)

ROUTER_PY=$(cat <<'EOF'
"""HTTP route dispatcher."""

from __future__ import annotations

import json
from typing import Dict, Tuple

from .service import add_item_payload, get_items_payload

Response = Tuple[int, Dict[str, object]]


def route_request(method: str, path: str, body: str) -> Response:
    if method == "GET" and path == "/health":
        return 200, {"status": "ok"}
    if method == "GET" and path == "/items":
        return 200, get_items_payload()
    if method == "POST" and path == "/items":
        payload = json.loads(body or "{}")
        name = payload.get("name", "")
        quantity = int(payload.get("quantity", 1))
        try:
            item = add_item_payload(name, quantity)
        except ValueError as error:
            return 400, {"error": str(error)}
        return 201, {"item": item}
    return 404, {"error": "not found"}
EOF
)

SERVER_PY=$(cat <<'EOF'
"""HTTP server wiring."""

from __future__ import annotations

import json
from http.server import BaseHTTPRequestHandler, HTTPServer

from .router import route_request


class ApiHandler(BaseHTTPRequestHandler):
    def _send(self, status: int, payload: dict) -> None:
        data = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self) -> None:
        status, payload = route_request("GET", self.path, "")
        self._send(status, payload)

    def do_POST(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8") if length else ""
        status, payload = route_request("POST", self.path, raw)
        self._send(status, payload)


def build_server(host: str = "127.0.0.1", port: int = 8055) -> HTTPServer:
    return HTTPServer((host, port), ApiHandler)
EOF
)

MAIN_PY=$(cat <<'EOF'
"""Application entrypoint."""

from hivemind_api.server import build_server


if __name__ == "__main__":
    server = build_server()
    print("Serving on http://127.0.0.1:8055")
    server.serve_forever()
EOF
)

TEST_SERVICE_PY=$(cat <<'EOF'
import unittest

from hivemind_api.service import add_item_payload, get_items_payload
from hivemind_api.store import reset_store


class ServiceTests(unittest.TestCase):
    def setUp(self) -> None:
        reset_store()

    def test_list_payload_has_count(self):
        payload = get_items_payload()
        self.assertIn("items", payload)
        self.assertEqual(payload["count"], 2)

    def test_add_item_updates_quantity(self):
        created = add_item_payload("gamma", 3)
        self.assertEqual(created["name"], "gamma")
        self.assertEqual(created["quantity"], 3)
        created = add_item_payload("gamma", 2)
        self.assertEqual(created["quantity"], 5)

    def test_add_item_rejects_blank_name(self):
        with self.assertRaises(ValueError):
            add_item_payload("   ", 1)


if __name__ == "__main__":
    unittest.main()
EOF
)

TEST_ROUTER_PY=$(cat <<'EOF'
import json
import unittest

from hivemind_api.router import route_request
from hivemind_api.store import reset_store


class RouterTests(unittest.TestCase):
    def setUp(self) -> None:
        reset_store()

    def test_health(self):
        status, payload = route_request("GET", "/health", "")
        self.assertEqual(status, 200)
        self.assertEqual(payload["status"], "ok")

    def test_items_round_trip(self):
        status, payload = route_request("POST", "/items", json.dumps({"name": "omega", "quantity": 4}))
        self.assertEqual(status, 201)
        self.assertEqual(payload["item"]["name"], "omega")
        status, payload = route_request("GET", "/items", "")
        self.assertEqual(status, 200)
        self.assertGreaterEqual(payload["count"], 3)

    def test_missing_item_name(self):
        status, payload = route_request("POST", "/items", "{}")
        self.assertEqual(status, 400)
        self.assertIn("error", payload)


if __name__ == "__main__":
    unittest.main()
EOF
)

README_MD=$(cat <<'EOF'
# Hivemind Native Runtime Beta API

This project is generated by Hivemind native runtime beta automation.

## Modules
- `hivemind_api/models.py`
- `hivemind_api/store.py`
- `hivemind_api/service.py`
- `hivemind_api/router.py`
- `hivemind_api/server.py`
- `main.py`

## Validation
Run:

```bash
python3 -m unittest discover -s tests -v
```
EOF
)

VERIFICATION_MD=$(cat <<'EOF'
# Verification Notes

- Runtime adapter: native (OpenRouter provider)
- Flow execution: two flows with inter-flow dependency
- Tool coverage: read_file, list_files, write_file, run_command, git_status, git_diff, graph_query
- Host validation command: `python3 -m unittest discover -s tests -v`
- Runtime hardening evidence: runtime state bootstrap + component readiness events
EOF
)

TASK_A_DIRECTIVES=(
  "ACT:tool:list_files:{\"path\":\".\",\"recursive\":false}"
  "$(mk_write_directive "hivemind_api/__init__.py" "$INIT_PY")"
  "$(mk_write_directive "hivemind_api/models.py" "$MODELS_PY")"
  "$(mk_write_directive "hivemind_api/store.py" "$STORE_PY")"
  "$(mk_write_directive "hivemind_api/service.py" "$SERVICE_PY")"
  "DONE:domain and storage modules created"
)

TASK_B_DIRECTIVES=(
  "ACT:tool:read_file:{\"path\":\"hivemind_api/service.py\"}"
  "$(mk_write_directive "hivemind_api/router.py" "$ROUTER_PY")"
  "$(mk_write_directive "hivemind_api/server.py" "$SERVER_PY")"
  "$(mk_write_directive "main.py" "$MAIN_PY")"
  "DONE:http routing and server wiring created"
)

TASK_C_DIRECTIVES=(
  "$(mk_write_directive "tests/test_service.py" "$TEST_SERVICE_PY")"
  "$(mk_write_directive "tests/test_router.py" "$TEST_ROUTER_PY")"
  "$(mk_write_directive "main.py" "$MAIN_PY")"
  "$(mk_write_directive "README.md" "$README_MD")"
  "ACT:tool:run_command:{\"command\":\"python3\",\"args\":[\"-m\",\"unittest\",\"discover\",\"-s\",\"tests\",\"-v\"]}"
  "ACT:tool:git_status:{}"
  "ACT:tool:git_diff:{\"staged\":false}"
  "ACT:tool:graph_query:{\"kind\":\"filter\",\"path_prefix\":\"hivemind_api\",\"max_results\":25}"
  "DONE:tests executed and runtime inspections completed"
)

TASK_D_DIRECTIVES=(
  "ACT:tool:read_file:{\"path\":\"README.md\"}"
  "$(mk_write_directive "docs/verification.md" "$VERIFICATION_MD")"
  "ACT:tool:run_command:{\"command\":\"python3\",\"args\":[\"-m\",\"unittest\",\"discover\",\"-s\",\"tests\",\"-v\"]}"
  "ACT:tool:git_status:{}"
  "ACT:tool:git_diff:{\"staged\":false}"
  "DONE:verification artifact and final checks completed"
)

TASK_A_PROMPT="$(build_prompt_from_directives "Build the API domain layer modules. Do not emit DONE until all steps are successful." "${TASK_A_DIRECTIVES[@]}")"
TASK_B_PROMPT="$(build_prompt_from_directives "Build the API HTTP layer modules. Follow steps exactly." "${TASK_B_DIRECTIVES[@]}")"
TASK_C_PROMPT="$(build_prompt_from_directives "Create tests/docs and run validation plus graph introspection. Follow steps exactly." "${TASK_C_DIRECTIVES[@]}")"
TASK_D_PROMPT="$(build_prompt_from_directives "Run flow-2 verification checks and produce docs/verification.md. Follow steps exactly." "${TASK_D_DIRECTIVES[@]}")"

echo "=== Creating tasks ==="
TASK_A_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Build domain layer" --description "$TASK_A_PROMPT")"
TASK_A_ID="$(printf '%s' "$TASK_A_JSON" | hm_id)"
TASK_B_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Build HTTP layer" --description "$TASK_B_PROMPT")"
TASK_B_ID="$(printf '%s' "$TASK_B_JSON" | hm_id)"
TASK_C_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Add tests and validation" --description "$TASK_C_PROMPT")"
TASK_C_ID="$(printf '%s' "$TASK_C_JSON" | hm_id)"
TASK_D_JSON="$(run_capture "$HIVEMIND" -f json task create "$PROJECT_NAME" "Flow 2 verification" --description "$TASK_D_PROMPT")"
TASK_D_ID="$(printf '%s' "$TASK_D_JSON" | hm_id)"
echo "TASK_A_ID=$TASK_A_ID"
echo "TASK_B_ID=$TASK_B_ID"
echo "TASK_C_ID=$TASK_C_ID"
echo "TASK_D_ID=$TASK_D_ID"

run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_A_ID" architecture-notes
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_B_ID" api-contract
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_C_ID" testing-policy
run "$HIVEMIND" project governance attachment include "$PROJECT_NAME" "$TASK_D_ID" testing-policy

echo "=== Creating graph and flow #1 ==="
GRAPH1_JSON="$(run_capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" phase45-native-beta-graph-1 --from-tasks "$TASK_A_ID" "$TASK_B_ID" "$TASK_C_ID")"
GRAPH1_ID="$(printf '%s' "$GRAPH1_JSON" | hm_id)"
run "$HIVEMIND" graph add-dependency "$GRAPH1_ID" "$TASK_A_ID" "$TASK_B_ID"
run "$HIVEMIND" graph add-dependency "$GRAPH1_ID" "$TASK_B_ID" "$TASK_C_ID"
run "$HIVEMIND" graph validate "$GRAPH1_ID"
FLOW1_JSON="$(run_capture "$HIVEMIND" -f json flow create "$GRAPH1_ID" --name phase45-native-flow-1)"
FLOW1_ID="$(printf '%s' "$FLOW1_JSON" | hm_id)"
echo "GRAPH1_ID=$GRAPH1_ID"
echo "FLOW1_ID=$FLOW1_ID"
run "$HIVEMIND" flow start "$FLOW1_ID"
run_flow_until_terminal "$FLOW1_ID" 120

echo "=== Creating graph and flow #2 with flow dependency ==="
GRAPH2_JSON="$(run_capture "$HIVEMIND" -f json graph create "$PROJECT_NAME" phase45-native-beta-graph-2 --from-tasks "$TASK_D_ID")"
GRAPH2_ID="$(printf '%s' "$GRAPH2_JSON" | hm_id)"
FLOW2_JSON="$(run_capture "$HIVEMIND" -f json flow create "$GRAPH2_ID" --name phase45-native-flow-2)"
FLOW2_ID="$(printf '%s' "$FLOW2_JSON" | hm_id)"
echo "GRAPH2_ID=$GRAPH2_ID"
echo "FLOW2_ID=$FLOW2_ID"
run "$HIVEMIND" flow add-dependency "$FLOW2_ID" "$FLOW1_ID"
run "$HIVEMIND" flow start "$FLOW2_ID"
run_flow_until_terminal "$FLOW2_ID" 120

FLOW1_STATUS="$("$HIVEMIND" -f json flow status "$FLOW1_ID" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
FLOW2_STATUS="$("$HIVEMIND" -f json flow status "$FLOW2_ID" | jq -r '.data.state' | tr '[:upper:]' '[:lower:]')"
[[ "$FLOW1_STATUS" == "completed" ]] || {
  echo "Flow1 not completed: $FLOW1_STATUS" >&2
  exit 1
}
[[ "$FLOW2_STATUS" == "completed" ]] || {
  echo "Flow2 not completed: $FLOW2_STATUS" >&2
  exit 1
}

echo "=== Merge completed flows ==="
run "$HIVEMIND" merge prepare "$FLOW1_ID"
run "$HIVEMIND" merge approve "$FLOW1_ID"
run "$HIVEMIND" merge execute "$FLOW1_ID" --mode local
run "$HIVEMIND" merge prepare "$FLOW2_ID"
run "$HIVEMIND" merge approve "$FLOW2_ID"
run "$HIVEMIND" merge execute "$FLOW2_ID" --mode local

echo "=== Host-side validation of merged API ==="
for required in \
  hivemind_api/models.py \
  hivemind_api/store.py \
  hivemind_api/service.py \
  hivemind_api/router.py \
  hivemind_api/server.py \
  tests/test_service.py \
  tests/test_router.py \
  docs/verification.md \
  main.py; do
  assert_file "$required"
done
run python3 -m unittest discover -s tests -v

echo "=== Event capture and assertions ==="
FLOW1_EVENTS="$(printf '%s' "$("$HIVEMIND" -f json events stream --flow "$FLOW1_ID" --limit 5000)" | redact_secret)"
FLOW2_EVENTS="$(printf '%s' "$("$HIVEMIND" -f json events stream --flow "$FLOW2_ID" --limit 3000)" | redact_secret)"
PROJECT_EVENTS="$(printf '%s' "$("$HIVEMIND" -f json events stream --project "$PROJECT_NAME" --limit 12000)" | redact_secret)"
printf '%s\n' "$FLOW1_EVENTS" > "$FLOW1_EVENTS_PATH"
printf '%s\n' "$FLOW2_EVENTS" > "$FLOW2_EVENTS_PATH"
printf '%s\n' "$PROJECT_EVENTS" > "$PROJECT_EVENTS_PATH"

PROJECT_TOOLS="$(printf '%s' "$PROJECT_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
FLOW1_TOOLS="$(printf '%s' "$FLOW1_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
FLOW2_TOOLS="$(printf '%s' "$FLOW2_EVENTS" | jq -r '.data[] | .payload | select(.type=="tool_call_completed") | .tool_name' | sort -u | tr '\n' ' ')"
echo "PROJECT_TOOLS=$PROJECT_TOOLS"
echo "FLOW1_TOOLS=$FLOW1_TOOLS"
echo "FLOW2_TOOLS=$FLOW2_TOOLS"

for tool in list_files write_file read_file run_command git_status git_diff graph_query; do
  assert_contains "$PROJECT_TOOLS" "$tool" "project tool coverage"
done

PROVIDER_LINE="$(printf '%s' "$FLOW1_EVENTS" | jq -r '[.data[] | .payload | select(.type=="agent_invocation_started") | (.provider + "|" + .model)][0] // empty')"
assert_contains "$PROVIDER_LINE" "openrouter|" "native provider/model provenance"
assert_contains "$PROVIDER_LINE" "$MODEL_ID" "native provider/model provenance"

RUNTIME_ENV_PAYLOAD="$(printf '%s' "$PROJECT_EVENTS" | jq -c '[.data[] | .payload | select(.type=="runtime_environment_prepared")][0] // empty')"
assert_contains "$RUNTIME_ENV_PAYLOAD" "\"inherit_mode\":\"all\"" "inherit mode all"
assert_contains "$RUNTIME_ENV_PAYLOAD" "BETA_PHASE45_SECRET_TOKEN" "dropped sensitive inherited key"
assert_contains "$RUNTIME_ENV_PAYLOAD" "HIVEMIND_RUNTIME_INTERNAL_PHASE45_RESERVED" "dropped reserved inherited key"
assert_contains "$RUNTIME_ENV_PAYLOAD" "OPENROUTER_API_KEY" "explicit sensitive overlay key"

assert_contains "$PROJECT_EVENTS" "\"strategy\": \"native_runtime_state_bootstrap\"" "runtime state bootstrap event"
assert_contains "$PROJECT_EVENTS" "\"strategy\": \"native_component_readiness\"" "readiness transition events"
assert_contains "$PROJECT_EVENTS" "\"type\": \"runtime_environment_prepared\"" "runtime env prepared event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"agent_invocation_started\"" "native invocation start event"
assert_contains "$PROJECT_EVENTS" "\"type\": \"agent_invocation_completed\"" "native invocation completed event"

ATTEMPTS_FLOW1="$("$HIVEMIND" -f json attempt list --flow "$FLOW1_ID" --limit 30)"
ATTEMPTS_FLOW2="$("$HIVEMIND" -f json attempt list --flow "$FLOW2_ID" --limit 30)"
FLOW1_ATTEMPTS_COUNT="$(printf '%s' "$ATTEMPTS_FLOW1" | jq '.data | length')"
FLOW2_ATTEMPTS_COUNT="$(printf '%s' "$ATTEMPTS_FLOW2" | jq '.data | length')"
PROJECT_EVENT_COUNT="$(printf '%s' "$PROJECT_EVENTS" | jq '.data | length')"
FLOW1_EVENT_COUNT="$(printf '%s' "$FLOW1_EVENTS" | jq '.data | length')"
FLOW2_EVENT_COUNT="$(printf '%s' "$FLOW2_EVENTS" | jq '.data | length')"

CONTEXT_PASS_COUNT=0
for attempt_id in $(printf '%s' "$ATTEMPTS_FLOW1" | jq -r '.data[].attempt_id') $(printf '%s' "$ATTEMPTS_FLOW2" | jq -r '.data[].attempt_id'); do
  [[ -n "$attempt_id" ]] || continue
  CTX_JSON="$("$HIVEMIND" -f json attempt inspect "$attempt_id" --context)"
  TEMPLATE_ID="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.template_id // empty')"
  SKILL_COUNT="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.skills | length')"
  DOC_COUNT="$(printf '%s' "$CTX_JSON" | jq -r '.context.manifest.documents | length')"
  if [[ "$TEMPLATE_ID" == "phase45-native-template" ]] && (( SKILL_COUNT >= 2 )) && (( DOC_COUNT >= 1 )); then
    CONTEXT_PASS_COUNT=$((CONTEXT_PASS_COUNT + 1))
  fi
done
if (( CONTEXT_PASS_COUNT == 0 )); then
  echo "No attempt context carried template/skills/documents as expected" >&2
  exit 1
fi

echo "=== Runtime state/secrets filesystem inspection ==="
STATE_DB_PATH="$TMP_BASE/native-state/native/runtime-state.sqlite"
SECRETS_STORE_PATH="$TMP_BASE/native-state/native/secrets.enc.json"
SECRETS_KEYRING_PATH="$TMP_BASE/native-state/native/secrets.keyring"
assert_file "$STATE_DB_PATH"
assert_file "$SECRETS_STORE_PATH"
if [[ -f "$SECRETS_KEYRING_PATH" ]]; then
  SECRETS_KEYRING_STATUS="present"
else
  SECRETS_KEYRING_STATUS="not_created_env_override"
fi

JOURNAL_MODE="$(sqlite3 "$STATE_DB_PATH" "PRAGMA journal_mode;")"
LOG_COUNT="$(sqlite3 "$STATE_DB_PATH" "SELECT COUNT(1) FROM runtime_logs;")"
LEASE_COUNT="$(sqlite3 "$STATE_DB_PATH" "SELECT COUNT(1) FROM job_leases;")"
assert_contains "$JOURNAL_MODE" "wal" "runtime state db wal mode"
if (( LOG_COUNT < 1 )); then
  echo "Expected runtime_logs rows in state DB" >&2
  exit 1
fi
if (( LEASE_COUNT < 0 )); then
  echo "Unexpected negative job_leases count in state DB" >&2
  exit 1
fi
if grep -Fq -- "$OPENROUTER_API_KEY" "$SECRETS_STORE_PATH"; then
  echo "Secrets store contains plaintext secret value; expected encrypted content" >&2
  exit 1
fi

cat > "$REPORT_PATH" <<EOF
# Phase 4.5 Native Runtime Beta Report ($DATE_TAG)

## Scope
- Phase 4.5 hardening validation (Sprints 52-57 implemented scope)
- Native runtime end-to-end beta using a substantial multi-module API project
- Two flow execution with inter-flow dependency, merge cycle, and host validation
- Unit matrix for startup hardening, env policy, sandbox/approval/network policy, transport retry/fallback, runtime durability/secrets/readiness

## Environment
- Hivemind binary: \`$HIVEMIND\`
- Runtime adapter/model: \`native\` / \`$MODEL_ID\`
- Project: \`$PROJECT_NAME\`
- Test repository: \`$PWD\`
- Data dir: \`$HIVEMIND_DATA_DIR\`

## IDs
- Flow 1: \`$FLOW1_ID\`
- Flow 2: \`$FLOW2_ID\`
- Tasks: \`$TASK_A_ID\`, \`$TASK_B_ID\`, \`$TASK_C_ID\`, \`$TASK_D_ID\`

## Beta Results
- Flow 1 status: \`$FLOW1_STATUS\`
- Flow 2 status: \`$FLOW2_STATUS\`
- Flow 1 attempts: \`$FLOW1_ATTEMPTS_COUNT\`
- Flow 2 attempts: \`$FLOW2_ATTEMPTS_COUNT\`
- Context pass-through checks: \`$CONTEXT_PASS_COUNT\`
- Provider/model observed: \`$PROVIDER_LINE\`

## Tool Coverage
- Project tools: \`$PROJECT_TOOLS\`
- Flow 1 tools: \`$FLOW1_TOOLS\`
- Flow 2 tools: \`$FLOW2_TOOLS\`

## Event Metrics
- Project events inspected: \`$PROJECT_EVENT_COUNT\`
- Flow 1 events inspected: \`$FLOW1_EVENT_COUNT\`
- Flow 2 events inspected: \`$FLOW2_EVENT_COUNT\`
- Runtime env payload sample: \`$RUNTIME_ENV_PAYLOAD\`

## Runtime State Durability Evidence
- State DB path: \`$STATE_DB_PATH\`
- SQLite journal mode: \`$JOURNAL_MODE\`
- runtime_logs rows: \`$LOG_COUNT\`
- job_leases rows: \`$LEASE_COUNT\`
- Secrets store path: \`$SECRETS_STORE_PATH\`
- Secrets keyring path: \`$SECRETS_KEYRING_PATH\` (\`$SECRETS_KEYRING_STATUS\`)
- Plaintext secret scan: \`PASS\`

## Host Validation
- \`python3 -m unittest discover -s tests -v\` passed after merging both flows.

## Artifacts
- Main log: \`$LOG_PATH\`
- Unit matrix log: \`$UNIT_LOG_PATH\`
- Flow 1 events: \`$FLOW1_EVENTS_PATH\`
- Flow 2 events: \`$FLOW2_EVENTS_PATH\`
- Project events: \`$PROJECT_EVENTS_PATH\`
- Report: \`$REPORT_PATH\`
EOF

echo
echo "BETA_TEST_RESULT=PASS"
echo "REPORT_PATH=$REPORT_PATH"
echo "LOG_PATH=$LOG_PATH"
echo "UNIT_LOG_PATH=$UNIT_LOG_PATH"
