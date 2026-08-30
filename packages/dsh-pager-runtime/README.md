# `@dsh-pager-grok/runtime` (source-only)

Controllers-v2 development Bundle for exact DSH `0.1.2-alpha.1`. This package
is private/source-only because its alpha dependencies are not registry
installable. Prepare it with `scripts/setup-dev-profile.sh`; the public CLI
does not depend on or attempt to publish it. Do not run its DSH profile on a
TTY — stdout is the JSON-RPC pipe.

Subpath exports:

- `@dsh-pager-grok/runtime/server` — Cordis TUI gateway plugin
