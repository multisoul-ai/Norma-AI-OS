# WorkspaceOverviewFloat 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-drawer-overview.png` | 左栏直接是运行概览，标题栏右侧有图钉；没有「对话 / 运行概览」分段 | 画布上的悬浮概览面板、分段标签条 |
| `02-floating-overview.png` | 画布右上方有圆角悬浮面板，标题是运行概览，带工作空间名、移动手柄和已按下的图钉；左栏仍是运行概览（Chat 隐藏后左栏不再切回对话） | 悬浮面板缺失、分段标签条 |
| `03-docked-overview.png` | 悬浮面板已收回（点悬浮面板图钉），左栏显示运行概览 | 画布上的悬浮面板，「和 Norma 聊聊」 |

对照 `docs/design-docs/assets/run-overview-prototype-floating-2026-10-19.png`：面板在画布右上、圆角深色玻璃、标题「运行概览」、右上图钉。真实 App 没有原型顶栏，也不重做概览行文案。
