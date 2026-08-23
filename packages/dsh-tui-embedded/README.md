# `@dsh-pager-grok/tui-embedded`

English | [中文](README.zh.md)

The external native-TUI backend bundle. [`cordis.patch.yml`](cordis.patch.yml) rides over the installed DSH `dsh-base`: it supplies the coding persona and tool mode, disables HMR so stdout stays a JSON-RPC pipe, mounts Code Mode's worker as a core execution capability, and inserts the host rows the out-of-process pager needs — workspace storage, the browse directory picker, local file-reference discovery, the transport-agnostic `api-gateway`, and the external `tui-server` on stdin/stdout. It mounts no HTTP server, Web runtime, or browser plugin. Agent tools stay in the host plane; session presets are a later overlay.

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
- **No agent-preset roster** — `agentPreset.*` RPCs have no roster in this composition; sessions use the host-plane tool set.
