# M0 Baseline Versions

> 固定日期：2026-08-23 +0800

M0 的来源和验证基线固定为：

| 项目 | 版本/来源 |
|---|---|
| Grok mirror commit | `19d42e35c07a9c9244f03f6df0c4c353f970d4f9` |
| Grok `SOURCE_REV` | `7d67deacbeb1c1093fdb4f9bcbfab2630e18a6aa` |
| DSH TUI protocol | `TUI_PROTOCOL_VERSION=1`; canonical fixtures checked against sibling `deepseek-harness` |
| Rust toolchain | `1.98.0`, declared in `rust-toolchain.toml` |
| Python fixture/PTY tools | Python `3.12` compatible standard-library scripts |
| Mock backend | Node `24` compatible `crates/dsh-pager-bin/tests/mock-server.mjs` |

Run `scripts/baseline.sh` from a clean checkout. It reports the actual source
revision and fails on protocol fixture drift, source hash drift, missing license,
format/check/test/clippy regressions, or PTY terminal restoration failure.
