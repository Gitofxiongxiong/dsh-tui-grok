# 外置包依赖锁文件闭合

## 目标

把 bundle 新增的公开 `dsh-api-remotes` 与 `dsh-file-reference` 依赖写入 pnpm lockfile，保证新 clone 使用 `pnpm install --frozen-lockfile` 时不重新解析依赖。

## 允许修改的文件

- `pnpm-lock.yaml`
- 本进度记录

## 原因

首次 workspace 验证在包 manifest 完成后发现这两个依赖需要由 pnpm 补入 importer；代码和 manifest 已在前一个迁移提交中固定，本批次只接受生成的 lockfile 增量。

## 验证

- `pnpm install`：lockfile passes supply-chain policies。
- `pnpm run verify:ts`：build、typecheck、protocol/server/bundle tests 全部通过。

提交 trailer：

`Progress-Record: docs/开发进度记录/2026-08-24_00-31-00_external_package_lock_closure.md`
