# 验证策略

## 快速检查

```bash
cargo check --workspace
cargo test --workspace
```

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
