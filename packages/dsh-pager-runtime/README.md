# `@dsh-pager-grok/runtime`

Single DeepSeek Harness Bundle for the native pager. Install it only through
`dsh plugin --profile dsh-pager-grok add @dsh-pager-grok/runtime` (the
`@dsh-pager-grok/cli` launcher does this). Do not run `dsh --profile
dsh-pager-grok` on a TTY — stdout is the JSON-RPC pipe.

Subpath exports:

- `@dsh-pager-grok/runtime/server` — Cordis TUI gateway plugin
- `@dsh-pager-grok/runtime/recovery` — session.list projection recovery plugin
