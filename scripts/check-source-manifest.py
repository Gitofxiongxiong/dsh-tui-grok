#!/usr/bin/env python3
"""Verify vendored Grok source provenance and licenses.

The check intentionally reports three independent classes of drift: the local
vendor file differs from the manifest, the upstream snapshot differs from the
manifest, and required license files are missing. A caller can therefore fix
provenance without changing vendor code to satisfy a lint gate.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path


ROW = re.compile(
    r"\|\s*`([^`]+)`\s*\|\s*([^|]+)\|\s*`([0-9a-f]{64})`\s*\|\s*`([0-9a-f]{64})`\s*\|\s*([^|]+)\|"
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def upstream_path(root: Path, relative: str) -> Path:
    relative_path = Path(relative)
    candidates = (root / relative_path, root / "crates" / "codegen" / relative_path)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    return candidates[-1]


def source_revision(root: Path) -> str | None:
    try:
        return subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--upstream",
        type=Path,
        default=Path("/home/leo/code/grok-build"),
        help="Grok Build checkout (default: /home/leo/code/grok-build)",
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    manifest_path = repo / "crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md"
    manifest = manifest_path.read_text(encoding="utf-8")

    local_drift: list[str] = []
    upstream_drift: list[str] = []
    missing_license: list[str] = []
    rows = list(ROW.finditer(manifest))
    if not rows:
        print(f"no manifest rows found: {manifest_path}", file=sys.stderr)
        return 1

    for row in rows:
        local_name, upstream_name, upstream_expected, vendored_expected, _change = row.groups()
        upstream_name = upstream_name.strip().strip("`")
        if upstream_name == "same":
            upstream_name = local_name.removeprefix("vendor/grok/")
        local_path = repo / "crates/dsh-pager-grok-ui" / local_name
        upstream = upstream_path(args.upstream, upstream_name)
        if not local_path.is_file():
            local_drift.append(f"missing local vendor file: {local_path}")
        else:
            actual = sha256(local_path)
            if actual != vendored_expected:
                local_drift.append(f"local hash drift: {local_name} ({actual})")
        if not upstream.is_file():
            upstream_drift.append(f"missing upstream file: {upstream}")
        else:
            actual = sha256(upstream)
            if actual != upstream_expected:
                upstream_drift.append(f"upstream hash drift: {upstream_name} ({actual})")

    license_candidates = (
        repo / "crates/dsh-pager-grok-ui/vendor/grok/LICENSE",
        args.upstream / "LICENSE",
    )
    for license_path in license_candidates:
        if not license_path.is_file():
            missing_license.append(f"missing license: {license_path}")

    revision = source_revision(args.upstream)
    if revision:
        print(f"upstream revision: {revision}")
    else:
        print(f"upstream revision unavailable: {args.upstream}")
    print(f"manifest rows checked: {len(rows)}")
    print(f"local drift: {len(local_drift)}")
    print(f"upstream drift: {len(upstream_drift)}")
    print(f"missing license: {len(missing_license)}")
    failures = local_drift + upstream_drift + missing_license
    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print("source manifest verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
