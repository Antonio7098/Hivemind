#!/usr/bin/env python3
"""Build a function dependency graph for Rust source files.

Usage examples:
  scripts/rust_fn_dependency_graph.py --source src/core/registry/flow.rs --entry create_flow --entry tick_flow
  scripts/rust_fn_dependency_graph.py --source src/cli/handlers/project.rs --entry handle_project --write-report
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, List, Set

ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = ROOT / "ops" / "migration_graphs"

FN_RE = re.compile(
    r"(?m)^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
)
CALL_RE = re.compile(r"\b([a-z_][a-zA-Z0-9_]*)\s*\(")

EXCLUDE_CALLS = {
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
    "Self",
}


@dataclass
class FnBlock:
    name: str
    text: str
    start: int
    end: int


def find_fn_block(src: str, name: str) -> FnBlock | None:
    m = re.search(
        rf"(?m)^\s*(?:pub(?:\([^\)]*\))?\s+)?(?:async\s+)?fn\s+{re.escape(name)}\s*\(",
        src,
    )
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
                return FnBlock(name=name, text=src[start : i + 1], start=start, end=i + 1)
        i += 1
    return None


def fn_names(src: str) -> Set[str]:
    return {m.group(1) for m in FN_RE.finditer(src)}


def fn_calls(text: str) -> Set[str]:
    names = {m.group(1) for m in CALL_RE.finditer(text)}
    return {n for n in names if n not in EXCLUDE_CALLS}


def build_graph(src: str, names: Set[str]) -> Dict[str, List[str]]:
    graph: Dict[str, List[str]] = {}
    for n in sorted(names):
        block = find_fn_block(src, n)
        if not block:
            graph[n] = []
            continue
        deps = sorted(c for c in fn_calls(block.text) if c in names and c != n)
        graph[n] = deps
    return graph


def closure(graph: Dict[str, List[str]], entry: List[str]) -> List[str]:
    seen: Set[str] = set()
    stack = list(entry)
    while stack:
        cur = stack.pop()
        if cur in seen:
            continue
        seen.add(cur)
        stack.extend(graph.get(cur, []))
    return sorted(seen)


def main() -> None:
    p = argparse.ArgumentParser(description="Rust function dependency graph")
    p.add_argument("--source", required=True, help="Rust source path")
    p.add_argument("--entry", action="append", default=[], help="Entry function name (repeatable)")
    p.add_argument("--write-report", action="store_true", help="Write JSON report to ops/migration_graphs")
    p.add_argument("--write-bundle", action="store_true", help="Write concatenated function bundle")
    args = p.parse_args()

    src_path = (ROOT / args.source).resolve() if not Path(args.source).is_absolute() else Path(args.source)
    src = src_path.read_text(encoding="utf-8")

    names = fn_names(src)
    graph = build_graph(src, names)

    missing_entry = [e for e in args.entry if e not in names]
    selected = closure(graph, args.entry) if args.entry else sorted(names)

    report = {
        "source": str(src_path.relative_to(ROOT)) if src_path.is_relative_to(ROOT) else str(src_path),
        "function_count": len(names),
        "entry": args.entry,
        "missing_entry": missing_entry,
        "selected_count": len(selected),
        "selected": selected,
        "graph": {k: v for k, v in graph.items() if k in selected} if args.entry else graph,
    }

    if args.write_report or args.write_bundle:
        OUT_DIR.mkdir(parents=True, exist_ok=True)

    if args.write_report:
        tag = src_path.name.replace(".", "_")
        out = OUT_DIR / f"{tag}.deps.json"
        out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {out}")

    if args.write_bundle:
        tag = src_path.name.replace(".", "_")
        out = OUT_DIR / f"{tag}.bundle.rs.txt"
        lines = [f"// source: {report['source']}", "// selected functions:"]
        for fn in selected:
            lines.append(f"// - {fn}")
        lines.append("")
        for fn in selected:
            block = find_fn_block(src, fn)
            if block:
                lines.append(block.text)
                lines.append("")
        out.write_text("\n".join(lines), encoding="utf-8")
        print(f"wrote {out}")

    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
