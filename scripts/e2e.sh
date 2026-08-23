#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

python3 scripts/check-protocol-fixtures.py
python3 scripts/check-source-manifest.py
python3 scripts/parity-matrix.py
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build -p dsh-pager-bin --locked
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager --full

echo "DSH/Grok M8-M10 end-to-end checks passed"
