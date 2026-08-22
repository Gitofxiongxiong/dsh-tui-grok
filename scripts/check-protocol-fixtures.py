#!/usr/bin/env python3
"""Check that Rust protocol fixtures mirror the Harness wire fixtures.

The Harness package owns the canonical JSON files. This check deliberately
compares parsed JSON rather than whitespace so formatting changes do not create
false drift while field additions or removals fail loudly.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def load(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"cannot read {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--harness",
        type=Path,
        default=Path(__file__).parents[2] / "deepseek-harness",
        help="deepseek-harness checkout (default: sibling repository)",
    )
    args = parser.parse_args()
    rust = Path(__file__).parents[1] / "crates/dsh-pager-protocol/tests/fixtures"
    harness = args.harness / "packages/tui/tui-protocol/tests/fixtures"
    names = ("hello-request.json", "hello-result.json")
    failures: list[str] = []
    for name in names:
        rust_path = rust / name
        harness_path = harness / name
        if not harness_path.exists():
            failures.append(f"missing Harness fixture: {harness_path}")
            continue
        if not rust_path.exists():
            failures.append(f"missing Rust fixture: {rust_path}")
            continue
        if load(rust_path) != load(harness_path):
            failures.append(f"wire fixture drift: {rust_path} != {harness_path}")
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"protocol fixtures in sync ({len(names)} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
