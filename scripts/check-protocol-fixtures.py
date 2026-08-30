#!/usr/bin/env python3
"""Check that the TypeScript protocol fixtures mirror Rust canonical JSON.

This check compares the complete JSON fixture file list and parsed values rather
than whitespace. Formatting-only changes do not create false drift, while file,
field, method, version, and error-kind drift fails with a concrete path.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


def load(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def differences(canonical: object, mirror: object, path: str = "$") -> list[str]:
    """Return JSON-path-oriented differences between two parsed values."""
    if type(canonical) is not type(mirror):
        return [
            f"{path}: type differs (canonical={type(canonical).__name__}, "
            f"mirror={type(mirror).__name__})"
        ]
    if isinstance(canonical, dict):
        result: list[str] = []
        canonical_keys = set(canonical)
        mirror_keys = set(mirror)
        for key in sorted(canonical_keys - mirror_keys):
            result.append(f"{path}.{key}: missing from mirror")
        for key in sorted(mirror_keys - canonical_keys):
            result.append(f"{path}.{key}: mirror-only field")
        for key in sorted(canonical_keys & mirror_keys):
            result.extend(differences(canonical[key], mirror[key], f"{path}.{key}"))
        return result
    if isinstance(canonical, list):
        result = []
        if len(canonical) != len(mirror):
            result.append(
                f"{path}: array length differs "
                f"(canonical={len(canonical)}, mirror={len(mirror)})"
            )
        for index, (canonical_item, mirror_item) in enumerate(zip(canonical, mirror)):
            result.extend(differences(canonical_item, mirror_item, f"{path}[{index}]"))
        return result
    if canonical != mirror:
        return [f"{path}: canonical={canonical!r}, mirror={mirror!r}"]
    return []


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--harness",
        type=Path,
        help="optional deepseek-harness checkout to compare against",
    )
    args = parser.parse_args()
    rust = Path(__file__).parents[1] / "crates/dsh-pager-protocol/tests/fixtures"
    mirror = (
        args.harness / "packages/tui/tui-protocol/tests/fixtures"
        if args.harness is not None
        else Path(__file__).parents[1] / "packages/dsh-tui-protocol/tests/fixtures"
    )
    failures: list[str] = []
    rust_names = {path.name for path in rust.glob("*.json")}
    mirror_names = {path.name for path in mirror.glob("*.json")}
    for name in sorted(rust_names - mirror_names):
        failures.append(f"missing mirror fixture: {mirror / name}")
    for name in sorted(mirror_names - rust_names):
        failures.append(f"mirror-only fixture: {mirror / name} (no canonical {rust / name})")

    for name in sorted(rust_names & mirror_names):
        rust_path = rust / name
        mirror_path = mirror / name
        try:
            canonical_value = load(rust_path)
        except (OSError, json.JSONDecodeError) as error:
            failures.append(f"cannot read canonical fixture {rust_path}: {error}")
            continue
        try:
            mirror_value = load(mirror_path)
        except (OSError, json.JSONDecodeError) as error:
            failures.append(f"cannot read mirror fixture {mirror_path}: {error}")
            continue
        for detail in differences(canonical_value, mirror_value):
            failures.append(
                f"wire fixture drift: {rust_path} != {mirror_path}: {detail}"
            )
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"protocol fixtures in sync ({len(rust_names)} JSON files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
