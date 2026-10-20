# WorkspaceStatusCapsule 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

空工作空间没有智能体，胶囊应保持隐藏。有智能体时只对照胶囊本身，不要求真实 App 出现原型顶栏。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-empty-capsule.png` | 工作空间名，画布右上角与抽屉重开按钮同一排 | 右上角状态胶囊、进度环、待处理角标、新增画布顶栏 |

有智能体时对照 `docs/design-docs/assets/run-overview-prototype-status-2026-10-19.png`：进度环、名字、已完成 n/m 或「待开始」、琥珀标记、最多 3 个、+N、待处理角标。对照 `run-overview-prototype-compact-2026-10-19.png`：左右抽屉都打开且画布变窄时只留环、数字和角标。

不确定时看同一次运行的 `app.log` 和 `report.json`，不要用相邻运行的截图补判。
