# 验证策略

## 快速检查

```bash
cargo check --workspace
cargo test --workspace
```

M0/M1 的可重建基线使用仓库脚本：

```bash
scripts/baseline.sh
```

它会校验 DSH wire fixture、Grok source manifest，运行 fmt/check/test/all-features
clippy，并用 checked-in mock backend 做 PTY 终端恢复 smoke。`scripts/test-fast.sh`
仍用于不编译 all-features 的快速循环。

固定来源、协议和工具版本见 [M0_BASELINE.md](M0_BASELINE.md)。

当前 `fixtures/parity/fallback-screen-80x24.json` 与
`fallback-screen-40x12.json` 是 M0 的简化 UI semantic baseline，明确标记为
fallback；它们不是 Grok reference golden，M10 reference runner 会替换它们。

## 分层测试

- Grok vendor 模块：保留上游已有单元测试，确认复制时行为不漂移。
- host adapter：用固定 `SessionState`/协议 fixture 验证 rows、状态和 effect 映射。
- runtime：继续运行协议、loader、control-plane、queue、lifecycle smoke。
- binary：`--hello`、`--load-only`、`--dashboard` 和各 smoke flag 不进入终端 UI。

## 终端检查

在支持 PTY 的环境运行：

```bash
cargo run -p dsh-pager-bin -- --backend <mock-server>
python3 scripts/pty-smoke.py --binary target/debug/dsh-pager
```

检查窗口 resize、折行、滚动、picker/modal 打开关闭以及退出后的终端恢复。UI 的快照/黄金测试只断言稳定文本和几何关系，不把 ANSI 控制序列硬编码到 runtime 测试。

## M8-M10 完整端到端门禁

scripts/e2e.sh 是当前完整入口：它校验协议/source manifest，验证
ReferenceRunner 的六档尺寸、十二种状态和九类输入矩阵，运行 workspace
测试，构建 binary，并通过 checked-in mock Harness 执行 PTY 主路径（含 resize、
mouse/picker、queue、Esc ladder 和 terminal restore）；prompt、selection/copy、
approval/question 和 queue authority 由同一 mock Harness 的 binary integration
tests 覆盖。真实 DeepSeek
Harness 可通过 DSH_TUI_SERVER 注入同一 binary；没有凭据或服务时脚本仍保留
mock 证据，并将真实后端状态记录为 unavailable，而不会伪造成功。
