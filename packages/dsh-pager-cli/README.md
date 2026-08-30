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

Use `dsh-pager doctor` for the selected DSH entry, family, profile schema,
capabilities, and distribution status. Do not run a family profile's raw `dsh`
backend on a TTY.
