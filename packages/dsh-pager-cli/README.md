# `@dsh-pager-grok/cli`

```bash
npm install -g @dsh-pager-grok/cli
dsh-pager
```

The CLI vendors the default supported DeepSeek Harness and pnpm, resolves the
exact DSH version through its bundled support registry, installs the matching
family runtime into a family profile (for example
`dsh-pager-grok-apiproxy-v1`), and spawns the platform-native pager. It does not
render the TUI and does not use a global `dsh` on PATH.

Running `dsh-pager` without a session flag starts a new conversation, matching
Grok Build. Use `dsh-pager --resume` for the most recent top-level conversation
in the current directory, `dsh-pager --resume <session-id>` for an exact
session, or `/resume` inside the TUI to pick from history. The older `--new`,
`--session`, and `--session-search` flags remain supported for compatibility.

Use `dsh-pager doctor` for the selected DSH entry, family, profile schema,
capabilities, and distribution status. Do not run a family profile's raw `dsh`
backend on a TTY.
