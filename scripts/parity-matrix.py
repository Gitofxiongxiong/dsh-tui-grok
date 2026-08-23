#!/usr/bin/env python3
"""Validate the reproducible M10 semantic parity matrix and fixtures."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

EXPECTED_SIZES = [[40, 12], [60, 20], [80, 24], [100, 30], [120, 40], [160, 50]]
REQUIRED_STATES = {
    "empty", "loading", "running", "streaming", "completed", "error",
    "reconnecting", "modal", "picker", "queue-edit", "selection", "dashboard-peek",
}
REQUIRED_INPUTS = {
    "key-repeat", "modified-key", "mouse-wheel", "mouse-click", "mouse-drag",
    "bracketed-paste", "resize-storm", "ctrl-c", "esc",
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path(__file__).parents[1]
        / "crates/dsh-pager-test-support/fixtures/parity/manifest.json",
    )
    parser.add_argument("--json", action="store_true", help="emit the validated matrix")
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text())
    matrix = manifest.get("referenceMatrix", {})
    sizes = matrix.get("sizes")
    states = set(matrix.get("states", []))
    inputs = set(matrix.get("inputs", []))
    if sizes != EXPECTED_SIZES:
        raise SystemExit(f"unexpected M10 dimensions: {sizes!r}")
    missing_states = REQUIRED_STATES - states
    missing_inputs = REQUIRED_INPUTS - inputs
    if missing_states or missing_inputs:
        raise SystemExit(
            f"matrix missing states={sorted(missing_states)} inputs={sorted(missing_inputs)}"
        )
    scenarios = manifest.get("scenarios", [])
    if not scenarios:
        raise SystemExit("parity manifest has no scenarios")
    missing_files = [
        scenario["fixture"]
        for scenario in scenarios
        if not (args.manifest.parent / scenario["fixture"]).is_file()
    ]
    if missing_files:
        raise SystemExit(f"missing parity fixtures: {missing_files}")
    result = {
        "status": matrix.get("status", "unknown"),
        "runner": matrix.get("runner"),
        "dimensions": sizes,
        "states": sorted(states),
        "inputs": sorted(inputs),
        "scenarioCount": len(scenarios),
        "caseCount": len(sizes) * len(states) * len(inputs),
    }
    if args.json:
        print(json.dumps(result, ensure_ascii=True, sort_keys=True))
    else:
        print(
            "M10 semantic parity matrix ok: "
            f"{result['caseCount']} cases, {result['scenarioCount']} fixtures, "
            f"runner={result['runner']}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
