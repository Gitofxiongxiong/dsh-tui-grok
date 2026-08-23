# @dsh-pager-grok/tui-server

English | [中文](README.zh.md)

Cordis plugin that serves the native TUI JSON-RPC protocol on caller-owned streams (production: stdin/stdout) and forwards business calls to `ctx.apiProxy`. Named exports only (`name`, `inject`, `Config`, `apply`); stdout is reserved for protocol frames.

## Behavior

- `tui.hello` assigns a client id and generation and reports the initial `resumeClass: baseline-required`; `tui.subscribe` then returns an explicit `session`, `control-plane`, or `all` scope with a `resume-accepted`/`baseline-required` classification and watermarks.
- `tui.attach` / `tui.subscribe` mark a session and start the mux/host pumps. Live mux frames for an attached session are buffered until `session.history` returns, then flushed in order; control frames for unattached sessions feed the bounded control-plane store and can fan out to observers.
- A baseline-required subscription emits `tui.controlPlaneBaseline` with per-session projection/queue/jobs/interaction snapshots, workspace/archive state, generation, and retained control records. Replays are limited by TTL/count bounds; slow clients may drop notifications and recover from the next baseline.
- A failed `session.history` result keeps that session's live backlog buffered; a repeated `tui.hello` starts a new generation and drops the previous generation's pumps and attachments.
- An unexpected normal close of either event stream is reported as `stream/error` so the client can enter reconnect instead of silently using a dead connection.
- ApiProxy unary methods (`session.list`, `session.prompt`, …) are forwarded through `toFetchHandler` so request schemas stay in the gateway package.
- `tui.respond` becomes `POST /api/respond`. An exact request-id replay returns the original receipt, while reusing that id with a different answer raises `already-resolved`.

`inject`: `apiProxy`. Runtime-only `input` / `output` overrides exist for tests; they are not Config fields.

## Model Experience

None, as this package is a client-facing presentation adapter; the model-visible surfaces belong to the runtime plugins behind ApiProxy.

#### KV Cache effect

None; this package neither assembles nor sends a provider request.

## Known Limitations and Deferred Work

- **One mux/host pump per connection** — presentation history remains a per-session load barrier; the control-plane store observes all sessions, while SharedAuto transport remains deferred.
- **No SharedAuto listener** — this plugin binds one stdio (or injected) stream pair; unix-socket connect-or-spawn belongs to a later transport provider.
- **Identity is not enforced** — hello may carry `TuiClientIdentity`, but this process does not compare profile, plugin digest, or sandbox against a shared-server key.
