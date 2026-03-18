#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORBIDDEN_LAYER_IMPORTS = {
    "src/core": ("crate::cli", "crate::server"),
    "src/storage": ("crate::cli", "crate::server"),
    "src/adapters": ("crate::cli", "crate::server"),
    "src/native": ("crate::cli", "crate::server"),
}
HOTSPOT_BUDGETS = {
    "src/core/registry/runtime/management/projection.rs": 1700,
    "src/server/routes/chat.rs": 980,
    "src/native/agent_loop.rs": 930,
    "src/native/tool_engine/tests.rs": 1000,
    "src/cli/handlers/events/native_summary.rs": 820,
    "src/native/turn_items.rs": 725,
    "src/native/tests.rs": 650,
    "src/adapters/opencode/runtime_impl.rs": 700,
    "src/core/registry/flow/execution/tick/once/runtime.rs": 640,
    "src/native/adapter/runtime.rs": 630,
    "src/server.rs": 520,
    "tests/governance.rs": 1900,
    "tests/events_cli.rs": 320,
    "tests/integration.rs": 2000,
    "tests/verification.rs": 900,
}
TOO_MANY_LINES_BUDGETS = {
    "src/adapters/opencode/interactive.rs": 1,
    "src/adapters/opencode/runtime_impl.rs": 1,
    "src/cli/handlers/attempt/runtime_data.rs": 1,
    "src/cli/handlers/events/labels.rs": 1,
    "src/cli/handlers/events/native_summary.rs": 2,
    "src/cli/handlers/flow.rs": 1,
    "src/cli/handlers/global/dispatch.rs": 1,
    "src/cli/handlers/global/skills.rs": 1,
    "src/cli/handlers/graph.rs": 1,
    "src/cli/handlers/project/governance.rs": 1,
    "src/cli/handlers/worktree.rs": 1,
    "src/core/context_window/pruning.rs": 1,
    "src/core/graph_query/index.rs": 1,
    "src/core/graph_query/query_engine/tests.rs": 1,
    "src/core/registry/events/recover.rs": 1,
    "src/core/registry/flow/checkpoint/completion.rs": 1,
    "src/core/registry/flow/execution/progress.rs": 1,
    "src/core/registry/flow/execution/tick/driver.rs": 1,
    "src/core/registry/flow/execution/tick/once/attempt.rs": 1,
    "src/core/registry/flow/execution/tick/once/runtime.rs": 1,
    "src/core/registry/flow/execution/tick/once.rs": 1,
    "src/core/registry/flow/management/lifecycle/create_start.rs": 1,
    "src/core/registry/flow/management/task_execution/retry.rs": 1,
    "src/core/registry/flow/management/task_execution/start.rs": 1,
    "src/core/registry/flow/management/task_execution/verify_override.rs": 1,
    "src/core/registry/flow/merge/execute/local.rs": 1,
    "src/core/registry/flow/merge/execute/pr.rs": 1,
    "src/core/registry/flow/merge/prepare/run/primary.rs": 1,
    "src/core/registry/flow/merge/prepare/run/secondary.rs": 1,
    "src/core/registry/flow/merge/prepare/run.rs": 1,
    "src/core/registry/flow/verification/process/task.rs": 1,
    "src/core/registry/governance/constitution/commands.rs": 2,
    "src/core/registry/governance/constitution/validate.rs": 1,
    "src/core/registry/governance/introspection/diagnose.rs": 1,
    "src/core/registry/governance/recovery/repair/actions.rs": 1,
    "src/core/registry/governance/recovery/repair/plan.rs": 1,
    "src/core/registry/governance/recovery/restore.rs": 1,
    "src/core/registry/governance/recovery/snapshot.rs": 1,
    "src/core/registry/graph/constitution/rules.rs": 1,
    "src/core/registry/graph/constitution/snapshot.rs": 1,
    "src/core/registry/graph/management/wiring.rs": 1,
    "src/core/registry/graph/snapshot/refresh.rs": 1,
    "src/core/registry/runtime/management/project.rs": 1,
    "src/core/state/apply/attempt.rs": 1,
    "src/core/state/apply/flow.rs": 1,
    "src/core/state/apply/graph.rs": 1,
    "src/core/state/catalog/governance.rs": 1,
    "src/core/state/catalog/runtime.rs": 1,
    "src/native/adapter/runtime.rs": 2,
    "src/native/prompt_assembly.rs": 1,
    "src/native/tool_engine/engine/dispatch.rs": 1,
    "src/native/tool_engine/exec_sessions/commands.rs": 1,
    "src/native/tool_engine/policy_eval.rs": 1,
    "src/native/tool_engine/run_command_tool.rs": 1,
    "src/native/turn_items.rs": 2,
    "src/server/event_ui/categories.rs": 1,
    "src/server/event_ui/types.rs": 1,
    "src/server/query_views.rs": 1,
    "src/server/routes/chat.rs": 1,
    "src/server/routes/queries.rs": 1,
    "src/server/tests.rs": 1,
    "src/server.rs": 1,
    "tests/governance.rs": 8,
    "tests/integration.rs": 1,
    "tests/verification.rs": 4,
}
MAX_TOO_MANY_LINES = sum(TOO_MANY_LINES_BUDGETS.values())
DEBT_TAG = "ARCH_DEBT"


def iter_rs_files(root: Path):
    for path in root.rglob("*.rs"):
        yield path


def is_comment(line: str) -> bool:
    stripped = line.lstrip()
    return stripped.startswith("//")


def count_lines(path: Path) -> int:
    with path.open("r", encoding="utf-8") as handle:
        return sum(1 for _ in handle)


def normalize_relative_path(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def has_debt_annotation(lines: list[str], index: int) -> bool:
    for back in range(1, 4):
        prev_index = index - back
        if prev_index < 0:
            break
        prev_line = lines[prev_index].strip()
        if not prev_line:
            continue
        if prev_line.startswith("//"):
            if DEBT_TAG in prev_line.upper():
                return True
            continue
        break
    return False


def main() -> int:
    errors: list[str] = []
    too_many_lines_count = 0
    too_many_lines_by_file: dict[str, int] = {}
    allow_pattern = re.compile(r"allow\(clippy::too_many_lines\)")

    for relative_root, forbidden_terms in FORBIDDEN_LAYER_IMPORTS.items():
        for path in iter_rs_files(ROOT / relative_root):
            lines = path.read_text(encoding="utf-8").splitlines()
            for number, line in enumerate(lines, start=1):
                if is_comment(line):
                    continue
                for term in forbidden_terms:
                    if term in line:
                        errors.append(f"{path.relative_to(ROOT)}:{number}: forbidden dependency on {term}")

    for path in iter_rs_files(ROOT / "src"):
        lines = path.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, start=1):
            if is_comment(line):
                continue
            if "Registry::open(" in line:
                errors.append(
                    f"{path.relative_to(ROOT)}:{number}: direct Registry::open() is forbidden; use AppContext"
                )
            if allow_pattern.search(line):
                if not has_debt_annotation(lines, number - 1):
                    errors.append(
                        (
                            f"{normalize_relative_path(path)}:{number}: add preceding '// {DEBT_TAG}: reason' comment "
                            "to treat this allowance as explicit architecture debt"
                        )
                    )
                too_many_lines_count += 1
                relative_path = normalize_relative_path(path)
                too_many_lines_by_file[relative_path] = too_many_lines_by_file.get(relative_path, 0) + 1

    for path in iter_rs_files(ROOT / "tests"):
        lines = path.read_text(encoding="utf-8").splitlines()
        for number, line in enumerate(lines, start=1):
            if allow_pattern.search(line):
                if not has_debt_annotation(lines, number - 1):
                    errors.append(
                        (
                            f"{normalize_relative_path(path)}:{number}: add preceding '// {DEBT_TAG}: reason' comment "
                            "to treat this allowance as explicit architecture debt"
                        )
                    )
                too_many_lines_count += 1
                relative_path = normalize_relative_path(path)
                too_many_lines_by_file[relative_path] = too_many_lines_by_file.get(relative_path, 0) + 1

    for relative_path, actual in sorted(too_many_lines_by_file.items()):
        budget = TOO_MANY_LINES_BUDGETS.get(relative_path)
        if budget is None:
            errors.append(
                f"{relative_path}: unexpected too_many_lines suppression count {actual}; add explicit budget"
            )
        elif actual > budget:
            errors.append(
                f"{relative_path}: too_many_lines suppression count {actual} exceeds budget {budget}"
            )

    for relative_path, budget in sorted(TOO_MANY_LINES_BUDGETS.items()):
        actual = too_many_lines_by_file.get(relative_path, 0)
        if actual > budget:
            errors.append(
                f"{relative_path}: too_many_lines suppression count {actual} exceeds budget {budget}"
            )

    if too_many_lines_count > MAX_TOO_MANY_LINES:
        errors.append(
            f"too_many_lines suppression count {too_many_lines_count} exceeds budget {MAX_TOO_MANY_LINES}"
        )

    for relative_path, budget in HOTSPOT_BUDGETS.items():
        path = ROOT / relative_path
        actual = count_lines(path)
        if actual > budget:
            errors.append(f"{relative_path}: {actual} lines exceeds hotspot budget {budget}")

    if errors:
        print("Architecture checks failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print("Architecture checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
