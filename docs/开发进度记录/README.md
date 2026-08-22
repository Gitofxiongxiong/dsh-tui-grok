# 开发进度记录

本目录是仓库变更的时间顺序审计轨迹。规则由
docs/GROK_BUILD_TUI_DEEPSEEK_ADAPTATION_PLAN.md 的“2.8 变更前进度记录契约”
和“2.9 Git 提交契约”定义，本文件提供可执行模板。

## 强制顺序

1. 可以先做只读搜索、分析、编译和测试；
2. 在第一次修改仓库前，创建一份新的时间戳文件；
3. 在记录中列出本次工作批次的目标文件和范围；
4. 只修改列出的范围；范围扩大前先创建下一份记录；
5. 完成验证后，把实际修改、命令结果、阻塞和后续事项补回同一份记录；
6. 暂存区审计通过后，用该记录的 trailer 创建一个对应 Git commit。

“仓库修改”包括代码、配置、文档、脚本、vendor、Cargo.lock、测试 fixture、
格式化结果和生成文件。不要先改目标文件再补日志。

## 文件名

格式：

~~~text
YYYY-MM-DD_HH-mm-ss_<简短主题>.md
~~~

示例：

~~~text
2026-08-22_22-59-46_变更记录规则.md
~~~

时间使用 Asia/Shanghai（+0800），精确到秒；文件名冲突时增加短序号或更具体
主题，不覆盖历史记录。

## 一份记录对应一个 commit

每个独立工作批次遵循一对一关系：

~~~text
一份主进度记录  <->  一个 Git commit
~~~

commit message 必须包含：

~~~text
Progress-Record: docs/开发进度记录/YYYY-MM-DD_HH-mm-ss_<简短主题>.md
~~~

提交前检查：

- 暂存区只包含该记录和记录中列出的目标文件；
- 没有第二份主进度记录、无关格式化或生成文件；
- commit message 的 trailer 路径与实际文件名完全一致；
- 如果工作必须拆成多个 commit，先为每个后续 commit 创建新记录；
- 如果工作需要 squash，先把记录范围合并为一份，再生成最终 commit。

记录与目标文件应在同一个 commit 中提交。不要为了事后写入 commit hash 而
制造额外提交；commit trailer、记录路径和提交文件列表共同构成可验证绑定。

当前 successor 尚没有 Git 元数据。Git 根建立后，M0.1 必须补充：

1. 检查所有历史记录的路径和命名；
2. 为提交钩子或 CI 增加 trailer/范围校验；
3. 在交接和发布检查中验证一对一关系。

## 记录模板

~~~markdown
# <主题>

> 记录时间：YYYY-MM-DD HH:MM:SS +0800
> 操作者/agent：
> 状态：planned / in-progress / completed / partial / blocked / canceled

## 目标与背景

## 设计契约和复用依据

- 对应长期计划章节：
- Grok source path、commit、SOURCE_REV：
- 复用等级 A0/A1/B/C/D：

## 计划修改范围

- 文件：
- 预期行为：
- 不在范围内：

## 风险、回滚和依赖

## 实际修改

- 文件：
- 摘要：

## 验证结果

- 命令：
- 结果：

## Git 提交

- commit message：
- Progress-Record trailer：
- 暂存区审计：

## 未解决问题和下一步
~~~

## 维护规则

- 历史记录不删除；发现错误时追加更正或新记录；
- 记录中的状态必须与实际结果一致；
- 记录不替代代码评审、测试报告、许可证或 source manifest；
- 一个记录对应一个清晰的工作批次，不把互不相关的功能混在一起。
