# dsh-pager-test-support

共享的、只用于测试的 dsh-pager 基础设施。它不进入生产依赖图，负责让不同
测试层级使用相同的资源所有权和 fixture 约定。

| 模块 | 作用 |
|---|---|
| `TestSandbox` | 私有临时 root、HOME、workspace、TMPDIR 和明确环境变量 |
| `read_jsonl` / `write_jsonl` | 稳定 JSONL fixture 读写与逐行错误诊断 |
| `Scenario` | 可序列化的命名步骤描述，供 deterministic integration/PTY 场景复用 |
| `run_with_timeout` | 有限时、必回收的普通子进程执行 |
| `normalize_ansi` / `visible_lines` | 终端输出的语义规范化，避免断言依赖光标控制字节 |

测试必须保持 sandbox 生命周期长于其子进程；不得修改进程全局环境。PTY 测试
需要更强的 screen/process-tree 控制时，应在此 crate 增加专用 owner，而不是在
单个测试中重复裸启动和清理逻辑。

新增模块时同步补充本 README、单元测试和一个实际 consumer。测试支持代码没有
覆盖理由时，不应成为只被声明却未被使用的抽象层。
