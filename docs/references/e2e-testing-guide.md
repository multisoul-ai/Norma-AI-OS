# E2E Testing Guide

本文件是 `crates/e2e` 的短导航。原始 app/headless GPUI e2e 目标方案背景见
[`docs/product-specs/2026-06-28-SPEC-e2e-testing.md`](../product-specs/2026-06-28-SPEC-e2e-testing.md)。
该 SPEC 仍是历史/目标方案，不等同于当前已经落地的最小 `crates/e2e`
层级。

## 当前层级

- `crates/e2e` 当前先承载 **domain-level e2e**：使用公开 crate API 串起跨 crate
  用户流程，不依赖 `crates/app` 的私有 `#[cfg(test)]` 入口。
- 原始 SPEC 中的 app/headless GPUI `crates/e2e` 方案尚未落地；当前先用
  domain-level e2e 补足可自动验证的公开 API 闭环。
- `crates/app/tests/unit/*` 仍承载 app-level headless `TestAppContext` 验收。
- 启动真实 packaged `.app` 的 L3 验收尚未落地，不能用 `cargo test -p e2e`
  代替真实语音、麦克风、扬声器和 OpenAI 联机验收。

## 写法约束

- 每条 e2e 测试必须有完整中文注释：场景、数据构造、执行过程、预期结果。
- 断言必须有失败说明；关键状态要有明确正断言和负断言。
- 优先测试公开 API 边界；不要为了 e2e 暴露 app 私有测试 helper。
- 若未来需要 app-level 独立 e2e，先把 `crates/app` 的可测试入口设计成稳定公开接口，
  不要从外部 crate 依赖 `pub(crate)` 或 `#[cfg(test)]` 细节。

## 常用命令

```bash
cargo test -p e2e
cargo test -p app app_realtime_acceptance_tests
```
