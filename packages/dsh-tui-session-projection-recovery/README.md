# @dsh-pager-grok/tui-session-projection-recovery

English | [中文](README.zh.md)

Cordis plugin that supplies the missing cold-row recovery path for
`session.list`. It decorates the public `ctx.apiProxy.sessions.list` seam; it
does not copy or patch DeepSeek Harness host source.

## Behavior

- Calls the already-composed host `session.list` first.
- Skips attached sessions and rows that already carry `projections`.
- For a detached row with a mounted `ctx.sessionProjectionCache`, calls
  `coldSnapshot(sessionId, signal)` in bounded batches of 16.
- Merges a non-empty snapshot into the row. The `sessionListMetadata` value can
  move `updatedAt` forward and clear `blank`, while a checkpoint saying blank
  remains only a prefix fact and never hides a visible row.
- Cache/log failures are fail-soft and leave the original row untouched;
  cancellation is propagated when a caller supplied an `AbortSignal`.

`coldSnapshot()` remains responsible for identity validation, projection fold,
and durable checkpoint write-back. The plugin owns no second cache and performs
no raw-log reads.

## Composition

Mount this plugin after `@deepseek-ai/dsh-host-apiproxy` and
`@deepseek-ai/dsh-session-projection-cache` in a profile patch. The
`dsh-tui-embedded` bundle in this repository mounts it automatically.

## Model Experience

None. This is a host-side read-model adapter and adds no provider prompt
content or model-visible events.

#### KV Cache effect

None. Durable checkpointing remains owned by the official projection-cache
service.

## Known Limitations

- The published `SessionsApi.list` contract has no cancellation parameter. The
  optional signal is honored when a direct in-process caller supplies one; the
  ordinary JSON-RPC carrier still uses the host's normal request lifetime.
- The plugin can only recover values exposed by the mounted projection-cache
  registry. If the cache or projection registry is absent, the original
  metadata-only row is returned.
