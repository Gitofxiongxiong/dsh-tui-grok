# 外置 profile 启动器与协议夹具自包含化

## 目标

迁移完成后，目标仓库的脚本和文档不再默认引用 Harness 内置的 `tui-embedded`（该内置 bundle 已不属于官方基线），协议夹具默认从本仓库外置 protocol 包读取；真实 E2E 通过显式的 `grok-tui` profile 或 `DSH_TUI_SERVER` 注入。

## 允许修改的文件

- `scripts/real-e2e.sh`
- `scripts/check-protocol-fixtures.py`
- `docs/TESTING.md`
- `README.md`
- 本进度记录

## 设计

- `DSH_TUI_PROFILE` 默认 `grok-tui`，保留 `DSH_TUI_SERVER` 完整命令覆盖。
- 协议 fixtures 的基准路径是本仓库 `packages/dsh-tui-protocol/tests/fixtures`；`--harness` 仍可用于和另一个 checkout 做显式比较。
- 不在脚本中写入凭据、修改 session 或执行 Harness 源码迁移。

## 验证与回滚

- 运行 `python3 scripts/check-protocol-fixtures.py`、`bash -n scripts/real-e2e.sh`。
- 运行 Rust fixture/unit 门禁；真实 E2E 仅在用户显式提供 profile/后端时运行。
- 回滚只需恢复本记录列出的五个文件，不触碰 Harness checkout。

提交 trailer：

`Progress-Record: docs/开发进度记录/2026-08-24_00-25-00_external_profile_launchers.md`

## 实际结果

- `real-e2e.sh` 默认 profile 改为 `grok-tui`，仍可由 `DSH_TUI_PROFILE` 或完整的 `DSH_TUI_SERVER` 覆盖。
- protocol fixture checker 默认使用本仓库 `packages/dsh-tui-protocol/tests/fixtures`，`--harness` 仅保留为显式外部对照模式。
- `bash -n scripts/real-e2e.sh`：通过；`python3 scripts/check-protocol-fixtures.py`：通过。
