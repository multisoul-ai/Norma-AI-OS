# WorkspaceDrawerTabs 视觉检查

运行 `bash scripts/desktop-e2e.sh workspace-drawer-tabs --visual`，由当前 Agent 看真实窗口原图。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `<theme>/01-run-overview.png` | 1280 点宽窗口的左栏右边缘约在 x=247（1280×31%−150=246.8）；左栏直接是运行概览，分区标题与按钮可读，中央画布从左栏右边缘开始 | 原约 397 点宽左栏、文字或按钮溢出、面板覆盖中央画布、Chat / Run Overview 分段、左栏对话输入框、悬浮概览面板 |
| `<theme>/02-chat-hidden.png` | 再按 ⌘⇧E 后保持运行概览和约 247 点宽度 | 分段标签条、对话页或左栏对话输入框 |
| `<theme>/03-idea-tooltip.png` | 完整正文自动换行，包含列表隐藏的末行，浅色与深色下文字清晰 | 被抽屉边缘裁切、只有截断标题、悬停改变布局宽度 |
| `<theme>/04-tooltip-dismissed.png` | 鼠标移出后提示消失，想法行仍可见 | 提示残留、想法被误删除或派发 |

主题分别为中文 `sapphire-night`、英文 `paper-blue`。不确定时查看同次运行的 `app.log` 与 `report.json`。
