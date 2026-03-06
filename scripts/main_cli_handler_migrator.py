#!/usr/bin/env python3
"""Migrate CLI handler functions from src/main.rs into src/cli/handlers/*.rs.

This script is intentionally conservative:
- extracts function blocks from main.rs by exact function name
- replaces same-named stub function blocks in handler files
- emits a dependency report for likely unresolved helper calls
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Tuple


ROOT = Path(__file__).resolve().parents[1]
MAIN = ROOT / "src/main.rs"
HANDLERS = ROOT / "src/cli/handlers"
OUT_DIR = ROOT / "ops/cli_migration"


MIGRATION_MAP: Dict[str, str] = {
    "handle_project": "project.rs",
    "handle_global": "global.rs",
    "handle_constitution": "governance.rs",
    "handle_events": "events.rs",
    "handle_verify": "verify.rs",
    "handle_merge": "merge.rs",
    "handle_attempt": "attempt.rs",
    "handle_checkpoint": "checkpoint.rs",
    "handle_task": "task.rs",
}


CALL_RE = re.compile(r"\b([a-z_][a-zA-Z0-9_]*)\s*\(")
FN_START_RE = re.compile(r"(?m)^fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(")


@dataclass
class ExtractedFn:
    name: str
    text: str
    start: int
    end: int


def find_fn_block(src: str, fn_name: str) -> ExtractedFn | None:
    m = re.search(rf"(?m)^(?:pub\s+)?fn\s+{re.escape(fn_name)}\s*\(", src)
    if not m:
        return None
    start = m.start()
    brace_start = src.find("{", m.end())
    if brace_start < 0:
        return None
    depth = 0
    i = brace_start
    while i < len(src):
        ch = src[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                return ExtractedFn(fn_name, src[start:end], start, end)
        i += 1
    return None


def replace_stub(handler_text: str, fn_name: str, replacement: str) -> Tuple[str, bool]:
    # Replace full function block with same name.
    found = find_fn_block(handler_text, fn_name)
    if not found:
        return handler_text, False
    new_text = handler_text[: found.start] + replacement + handler_text[found.end :]
    return new_text, True


def find_main_fn_names(main_text: str) -> set[str]:
    return {m.group(1) for m in FN_START_RE.finditer(main_text)}


def unresolved_calls(fn_text: str, main_fn_names: set[str]) -> List[str]:
    keywords = {
        "if",
        "match",
        "for",
        "while",
        "loop",
        "return",
        "println",
        "print",
        "format",
        "Ok",
        "Err",
        "Some",
        "None",
        "vec",
        "String",
    }
    called = {m.group(1) for m in CALL_RE.finditer(fn_text)}
    unresolved = sorted(
        name
        for name in called
        if name in main_fn_names and name not in keywords and not name.startswith("handle_")
    )
    return unresolved


def main() -> None:
    parser = argparse.ArgumentParser(description="Migrate main.rs handler functions into cli/handlers")
    parser.add_argument("--apply", action="store_true", help="Apply replacements to handler files")
    parser.add_argument("--write-report", action="store_true", help="Write JSON report under ops/cli_migration")
    parser.add_argument(
        "--function",
        action="append",
        default=[],
        help="Only process selected function(s), repeatable (e.g. --function handle_global)",
    )
    parser.add_argument(
        "--write-bundles",
        action="store_true",
        help="Write extracted function bundles under ops/cli_migration/bundles",
    )
    args = parser.parse_args()

    main_text = MAIN.read_text(encoding="utf-8")
    main_fn_names = find_main_fn_names(main_text)

    report: Dict[str, object] = {"migrations": []}
    updates: List[Tuple[Path, str]] = []

    selected = set(args.function) if args.function else None
    for fn_name, handler_file in MIGRATION_MAP.items():
        if selected is not None and fn_name not in selected:
            continue
        extracted = find_fn_block(main_text, fn_name)
        handler_path = HANDLERS / handler_file
        status = {
            "function": fn_name,
            "handler_file": str(handler_path.relative_to(ROOT)),
            "found_in_main": bool(extracted),
            "replaced": False,
            "unresolved_helper_calls": [],
        }

        if extracted and handler_path.exists():
            handler_text = handler_path.read_text(encoding="utf-8")
            replaced_text, replaced = replace_stub(handler_text, fn_name, extracted.text)
            status["replaced"] = replaced
            status["unresolved_helper_calls"] = unresolved_calls(extracted.text, main_fn_names)
            if replaced:
                updates.append((handler_path, replaced_text))
            if args.write_bundles:
                bundle_dir = OUT_DIR / "bundles"
                bundle_dir.mkdir(parents=True, exist_ok=True)
                bundle_path = bundle_dir / f"{fn_name}.rs.txt"
                header = [
                    f"// function: {fn_name}",
                    f"// target: {handler_path.relative_to(ROOT)}",
                    "// unresolved helpers (main.rs-local):",
                ]
                helpers = status["unresolved_helper_calls"] or []
                for h in helpers:
                    header.append(f"// - {h}")
                bundle_path.write_text(
                    "\n".join(header) + "\n\n" + extracted.text + "\n",
                    encoding="utf-8",
                )

        report["migrations"].append(status)

    if args.apply:
        for path, text in updates:
            path.write_text(text, encoding="utf-8")

    if args.write_report:
        OUT_DIR.mkdir(parents=True, exist_ok=True)
        out = OUT_DIR / "main_cli_handler_migration_report.json"
        out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Wrote report: {out}")

    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
