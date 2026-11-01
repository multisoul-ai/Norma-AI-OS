---
name: norma-desktop-e2e
description: 为本次 UI 或用户流程差异补充真实 macOS Norma App 验收证据，优先运行最小 desktop-e2e 场景；也用于新增场景、前置检查失败和 ENV/PRODUCT 分诊。合并任务先按执行规范判断已有证据是否可复用。
---

# 桌面 E2E

先按[执行规范](../../../docs/quality/agent-execution-policy.md)确定本次验收范围与可复用证据；需要补充真实窗口证据时使用本技能。细则在 [`docs/quality/desktop-e2e.md`](../../../docs/quality/desktop-e2e.md)。

## 步骤

1. 读 [`references/preflight.md`](references/preflight.md)。锁屏或控制通道不可用时停下来说明，等用户处理后再跑。
2. 确认被测二进制是 `target/debug/norma-os`，不是 `/Applications/Norma OS.app`。不要结束用户已经打开的 Norma。
3. 按 [`references/modules.md`](references/modules.md) 选择覆盖本次差异的最小现有场景，已接入模型看图的场景优先用 `bash scripts/desktop-e2e.sh <场景> --visual`，按 [`references/model-vision.md`](references/model-vision.md) 实际读图并逐项响应；其他场景用 `bash scripts/desktop-e2e.sh <场景>` 执行；完整模块验收才用 `<模块>` 跑全部场景。需要验证稳定性时加 `--repeat 5`。默认在私有虚拟显示器后台运行；用户要亲自观看或验收时，告诉用户用 `NORMA_E2E_VISIBLE=1`（或 `NORMA_E2E_FOREGROUND=1`）自行运行，见 [desktop-e2e.md](../../../docs/quality/desktop-e2e.md)“关闭后台模式”。
3a. 默认走模型多模态看图：场景尚未接入时，先在模块 `visualScenarioNames` 补看图版（`session.visualClick` / `visualCheck` / `capturePixels`），不要依赖系统 Vision OCR。
4. 退出码 `3` 按 [`references/triage.md`](references/triage.md) 记为 ENV 并停止；若原因是 Vision OCR 不可用（e5rt 错误），改用该场景的 `--visual`，不得降级 Computer Use。退出码 `1` 记为 PRODUCT，附 `report.json`、失败截图和 `app.log`。
5. 从 `checks/<模块>/*.checks.md` 选取本次范围内的关键图并查看，不因场景额外产图扩大读图范围。每张给符合、不符合或不确定；方式见 [`references/visual-checks.md`](references/visual-checks.md)。
6. 记录本次范围、复用证据、实际运行的退出码/耗时/通过率，以及必要的视觉结论与证据路径。面向用户汇报摘要，详细记录按需链接。

完成标准：本次范围内的硬断言与必要视觉检查均有结论，复用证据有来源；只对已验证范围声明通过。完整场景验收须覆盖其全部关键项。

## 红线

- 只结束 `artifacts/desktop-workspace-e2e/owned.json` 记录的、且路径是本次测试二进制的 PID。
- 不运行已删除的 `crates/e2e`。
- 不把截图文件存在、headless GPUI 或单测写成真实窗口验收。
- 场景不为测试改产品行为。App 侧只允许 debug 专用的测试控制通道（`crates/app/src/e2e_control/`），它只提供原始输入与本窗口截图，不暴露内部状态。

新增场景时先读 [`references/modules.md`](references/modules.md)，再按 [`references/add-scenario.md`](references/add-scenario.md) 放到对应模块。
