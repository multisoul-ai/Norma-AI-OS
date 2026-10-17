---
name: norma-desktop-e2e
description: 在真实 macOS Norma App 上验收页面、布局、样式和用户流程。先跑 scripts/desktop-e2e.sh；场景覆盖不到时再降级 Computer Use。也用于按模块新增桌面场景、前置检查失败，以及区分 ENV 与 PRODUCT。
---

# 桌面 E2E

页面或用户流程要在真实 Norma 窗口上验收时用本技能。细则在 [`docs/quality/desktop-e2e.md`](../../../docs/quality/desktop-e2e.md)。

## 步骤

1. 读 [`references/preflight.md`](references/preflight.md)。锁屏、辅助功能或屏幕录制未就绪时停下来说明，等用户处理后再跑。
2. 确认被测二进制是 `target/debug/norma-os`，不是 `/Applications/Norma OS.app`。不要结束用户已经打开的 Norma。
3. 按 [`references/modules.md`](references/modules.md) 选择模块或场景。`bash scripts/desktop-e2e.sh <模块>` 跑该模块全部场景；`bash scripts/desktop-e2e.sh <场景>` 只跑一条。工作空间增删改查用 `workspace` 或 `workspace-lifecycle`。需要稳定性时加 `--repeat 5`。
4. 退出码 `3` 按 [`references/triage.md`](references/triage.md) 记为 ENV 并停止。退出码 `1` 记为 PRODUCT，附 `report.json`、失败截图和 `app.log`。
5. 按该场景的 `checks/<模块>/*.checks.md` 逐张打开关键截图。看图方式见 [`references/visual-checks.md`](references/visual-checks.md)。每张给符合、不符合或不确定。
6. 汇报：每步通过或失败、总耗时、截图绝对路径、连跑通过率、ENV 或 PRODUCT。

完成标准：命令退出码、SQLite 和文件断言、以及每张关键截图的视觉结论都写进汇报。缺任何一项就还没验收完。

## 红线

- 只结束 `artifacts/desktop-workspace-e2e/owned.json` 记录的、且路径是本次测试二进制的 PID。
- 不运行已删除的 `crates/e2e`。
- 不把截图文件存在、headless GPUI 或单测写成真实窗口验收。
- P0 不改产品代码，不加 App 测试钩子。

新增场景时先读 [`references/modules.md`](references/modules.md)，再按 [`references/add-scenario.md`](references/add-scenario.md) 放到对应模块。
