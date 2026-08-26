# `@dsh-pager-grok/native-*`

Prebuilt `dsh-pager` binaries for the v2 product launcher. Each package
contains **one** stripped executable and is selected by npm `os` / `cpu` /
`libc` metadata.

| Package | Target |
|---|---|
| `@dsh-pager-grok/native-linux-x64-gnu` | `x86_64-unknown-linux-gnu` |
| `@dsh-pager-grok/native-linux-arm64-gnu` | `aarch64-unknown-linux-gnu` |
| `@dsh-pager-grok/native-darwin-x64` | `x86_64-apple-darwin` |
| `@dsh-pager-grok/native-darwin-arm64` | `aarch64-apple-darwin` |
| `@dsh-pager-grok/native-win32-x64` | `x86_64-pc-windows-msvc` |

These packages have **no** `bin` field. The future `@dsh-pager-grok/cli`
resolves the matching optional dependency and spawns it. musl/Alpine and
Windows ARM are unsupported.

Binaries are produced by `node scripts/pack-native.mjs` / the release
workflow and are **not** stored in git. Pack with `npm pack` (not `pnpm pack`)
so the Unix executable bit survives.
