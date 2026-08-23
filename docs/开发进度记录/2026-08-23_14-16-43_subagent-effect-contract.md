# Subagent interrupt effect contract

时间：2026-08-23 14:16:43 +0800

## 目标

将 Agent Tasks overlay 的 interrupt 从 runtime 直接 RPC 调用收敛到统一的
`UiIntent -> UiEffect -> UiEffectReceipt` seam。

## 本批范围

- 新增 `InterruptSubagent` 中性 intent/effect，携带 parent/child/mode 稳定地址。
- `DshEffectSink` 调用 Harness `subagent.interrupt`，只将 admission receipt 返回给 UI；
  stopped/complete 仍以之后的 host snapshot 为准。
- 保留 one-shot mode 的 host rejection，并用 operation/generation/dedupe identity
  保护重试和 late response。

## 下一批

以同一 effect seam 接入 file-search query、media preview 和 workspace mutation，
然后建立真实 Grok reference runner / backend matrix。
