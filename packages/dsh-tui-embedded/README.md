# `@dsh-pager-grok/tui-embedded`

English | [中文](README.zh.md)

The external native-TUI backend bundle. [`cordis.patch.yml`](cordis.patch.yml) rides over the installed DSH `dsh-base`: it supplies the coding persona and tool mode, disables HMR so stdout stays a JSON-RPC pipe, mounts Code Mode's worker as a core execution capability, and inserts the direct session/settings/workspace controllers plus the external `tui-server` on stdin/stdout. Harness 0.1.2 owns storage and cold-session inspection in its base profile, so this overlay does not duplicate storage/projection rows or mount a recovery adapter. It mounts no HTTP server, Web runtime, or browser plugin. Model-facing tools come from a per-session agent preset (`standard` / `ptc` / `minimal` / `cordis`).

Install this bundle into a custom profile such as `grok-tui` with `dsh plugin --profile grok-tui add @dsh-pager-grok/tui-embedded`, then run `dsh --profile grok-tui`. The native pager (`dsh-pager`) speaks `tui.hello` on the pipe. Stdout is reserved for protocol frames; diagnostics belong on stderr.

The package has no runtime API; the profile composer resolves the patch through the `dsh.bundle.patch` manifest field, never through code.

## Model Experience

None, as this bundle only mounts the stdio TUI gateway and host rows; prompts and tools belong to the composed base and host plugins.

#### KV Cache effect

None; the bundle adds no tokens to a provider request.

## Known Limitations and Deferred Work

- **Stdout is the protocol pipe** — any plugin that writes to stdout corrupts the TUI framing; HMR is disabled for that reason, and this bundle mounts no logger.
- **Browse directory picker only** — `host.pickDirectory` has no native dialog; the pager lists through `host.listDirectory`.
- **No SharedAuto listener** — this process serves one stdio client; unix-socket connect-or-spawn is a later transport.
- **Agent presets are session-bound** — `session.create` / `agentPreset.select` compose one roster preset per conversation. A started session cannot switch (`agent-preset-locked`). Per-session Code Mode is the `ptc` preset; `DSH_TOOLS_MODE` remains a process-wide escape hatch.
