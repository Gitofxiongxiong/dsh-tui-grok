# `@dsh-pager-grok/cli`

```bash
npm install -g @dsh-pager-grok/cli
dsh-pager
```

The CLI vendors a pinned DeepSeek Harness and pnpm, installs
`@dsh-pager-grok/runtime` into profile `dsh-pager-grok`, and spawns the
platform-native pager. It does not render the TUI and does not use a global
`dsh` on PATH.

Do not run `dsh --profile dsh-pager-grok` on a TTY.
