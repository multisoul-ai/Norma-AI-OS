# WorkspaceDrawerTabs 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-run-overview.png` | 左栏直接是运行概览（标题「Run Overview」、需要你处理、当前项目、想法）；没有「Chat / Run Overview」分段；左栏底部没有「Talk to Norma」对话输入框 | 分段标签条、对话输入框、悬浮概览面板 |
| `02-chat-hidden.png` | 再次 ⌘⇧E 后左栏仍是运行概览，布局与上一张一致 | 分段标签条、对话页、「和 Norma 聊聊」、左栏底部对话输入框 |

不确定时看同一次运行的 `app.log` 和 `report.json`，不要用相邻运行的截图补判。
