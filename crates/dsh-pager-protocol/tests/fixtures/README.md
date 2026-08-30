# TUI protocol fixtures

This directory is the canonical fixture catalog for TUI wire protocol version 1.
packages/dsh-tui-protocol/tests/fixtures is its TypeScript mirror. Add or edit
JSON here first, copy the same JSON to the mirror, then run
python3 scripts/check-protocol-fixtures.py.

The mirror check compares the complete *.json file list and parsed JSON values.
This README documents ownership and is not itself a mirrored fixture.

## Ownership boundary

The Rust protocol crate owns the JSON-RPC line carrier, protocol version, typed
hello/attach/subscribe/respond payloads, and typed business DTOs. It does not
currently define exhaustive Rust discriminated unions for MuxFrame or HostFrame.
Those notification payloads remain JSON values interpreted by the Rust pager
reducers according to their type field.

Until reliable cross-language code generation exists, mux/host frame semantics
are anchored by this fixture catalog and tests on both sides. Phase 2 deliberately
does not add parallel hand-written Rust frame unions. Representative mux/host
frame fixtures will be added with the Phase 3 adapter conformance suite.
