#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 scripts/check-protocol-fixtures.py
python3 scripts/check-source-manifest.py
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features -- -D warnings

# The PTY check uses only the checked-in protocol mock and verifies alternate
# screen/raw-mode restoration, picker routing, and clean process shutdown.
cargo build -p dsh-pager-bin --locked
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager

echo "DSH/Grok M0 baseline passed"
