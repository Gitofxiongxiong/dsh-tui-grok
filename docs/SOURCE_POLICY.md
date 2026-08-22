# 源码复用与许可证策略

## 复用原则

能直接复用 Grok Build 的代码就不重写。复制范围优先覆盖用户可见的 view、交互状态和布局；runtime shell、agent/tool orchestration、配置系统和 Grok 专属业务协议不复制。

## Vendor 规则

- 所有复制文件放在 `crates/dsh-pager-grok-ui/vendor/grok/`，按上游模块路径保存。
- `crates/dsh-pager-grok-ui/SOURCE_MANIFEST.md` 记录上游仓库、commit、原始路径、hash 和本地修改原因。
- 上游升级采用一次性同步和测试，不在多个 crate 里复制第二份。
- 适配逻辑放在 `src/`，不直接改写 host runtime；确需改动复制文件时保留短注释并更新 manifest。

## 许可证

上游许可证随 vendor 源码保留。项目发布时同时携带根目录许可证、`NOTICE` 以及 vendor 许可证；新增依赖按其许可证兼容性审核。
