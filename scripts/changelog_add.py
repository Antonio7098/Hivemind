#!/usr/bin/env python3
"""
Changelog helper for Hivemind.

Usage:
    python scripts/changelog_add.py --interactive
    python scripts/changelog_add.py \
        --version "phase-1" \
        --type added \
        --area core \
        --description "Event foundation with append-only event store" \
        --phase 1
"""

import json
import argparse
from datetime import datetime
from pathlib import Path

CHANGELOG_PATH = Path(__file__).parent.parent / "changelog.json"

VALID_TYPES = ["added", "changed", "fixed", "removed", "deprecated", "security"]
VALID_AREAS = ["core", "cli", "adapters", "storage", "docs", "tests", "ci", "other"]


def load_changelog():
    """Load the changelog.json file."""
    if not CHANGELOG_PATH.exists():
        return {
            "project": "hivemind",
            "description": "Changelog for Hivemind",
            "format_version": "1.0.0",
            "entries": []
        }
    with open(CHANGELOG_PATH, "r") as f:
        return json.load(f)


def save_changelog(data):
    """Save the changelog.json file."""
    with open(CHANGELOG_PATH, "w") as f:
        json.dump(data, f, indent=2)
    print(f"Changelog saved to {CHANGELOG_PATH}")


def add_entry(version, date, changes, phase=None):
    """Add a new entry to the changelog."""
    changelog = load_changelog()

    entry = {
        "version": version,
        "date": date,
        "changes": changes
    }

    if phase is not None:
        entry["phase"] = phase

    # Insert at the beginning
    changelog["entries"].insert(0, entry)

    save_changelog(changelog)
    return entry


def interactive_mode():
    """Interactive mode for adding changelog entries."""
    print("=== Hivemind Changelog Entry ===\n")

    version = input("Version (e.g., 'phase-1' or 'v0.1.0'): ").strip()
    date = input(f"Date [{datetime.now().strftime('%Y-%m-%d')}]: ").strip()
    if not date:
        date = datetime.now().strftime("%Y-%m-%d")

    phase_input = input("Phase number (or press Enter to skip): ").strip()
    phase = int(phase_input) if phase_input else None

    changes = []
    print("\nAdd changes (empty description to finish):\n")

    while True:
        print(f"  Types: {', '.join(VALID_TYPES)}")
        change_type = input("  Type: ").strip().lower()
        if not change_type:
            break
        if change_type not in VALID_TYPES:
            print(f"  Invalid type. Choose from: {', '.join(VALID_TYPES)}")
            continue

        print(f"  Areas: {', '.join(VALID_AREAS)}")
        area = input("  Area: ").strip().lower()
        if area not in VALID_AREAS:
            print(f"  Invalid area. Choose from: {', '.join(VALID_AREAS)}")
            continue

        description = input("  Description: ").strip()
        if not description:
            break

        commit = input("  Commit hash (optional): ").strip() or None
        files = input("  Files affected (comma-separated, optional): ").strip()
        files_list = [f.strip() for f in files.split(",")] if files else None
        issues = input("  Related issues (comma-separated, optional): ").strip()
        issues_list = [i.strip() for i in issues.split(",")] if issues else None

        change = {
            "type": change_type,
            "area": area,
            "description": description
        }
        if commit:
            change["commit"] = commit
        if files_list:
            change["files"] = files_list
        if issues_list:
            change["issues"] = issues_list

        changes.append(change)
        print("  Change added.\n")

    if changes:
        entry = add_entry(version, date, changes, phase)
        print(f"\nEntry added: {json.dumps(entry, indent=2)}")
    else:
        print("\nNo changes added. Exiting.")


def cli_mode(args):
    """Command-line mode for adding a single change."""
    change = {
        "type": args.type,
        "area": args.area,
        "description": args.description
    }

    if args.commit:
        change["commit"] = args.commit
    if args.files:
        change["files"] = args.files
    if args.issues:
        change["issues"] = args.issues

    date = args.date or datetime.now().strftime("%Y-%m-%d")

    entry = add_entry(args.version, date, [change], args.phase)
    print(f"Entry added: {json.dumps(entry, indent=2)}")


def main():
    parser = argparse.ArgumentParser(description="Add entries to Hivemind changelog")
    parser.add_argument("--interactive", "-i", action="store_true",
                        help="Interactive mode")
    parser.add_argument("--version", "-v", help="Version string (e.g., 'phase-1')")
    parser.add_argument("--date", "-d", help="Date (YYYY-MM-DD), defaults to today")
    parser.add_argument("--type", "-t", choices=VALID_TYPES,
                        help="Change type")
    parser.add_argument("--area", "-a", choices=VALID_AREAS,
                        help="Affected area")
    parser.add_argument("--description", help="Change description")
    parser.add_argument("--phase", "-p", type=int, help="Phase number")
    parser.add_argument("--commit", "-c", help="Commit hash")
    parser.add_argument("--files", "-f", nargs="+", help="Affected files")
    parser.add_argument("--issues", nargs="+", help="Related issues")

    args = parser.parse_args()

    if args.interactive:
        interactive_mode()
    elif args.version and args.type and args.area and args.description:
        cli_mode(args)
    else:
        parser.print_help()
        print("\nUse --interactive for guided entry, or provide all required flags.")


if __name__ == "__main__":
    main()
