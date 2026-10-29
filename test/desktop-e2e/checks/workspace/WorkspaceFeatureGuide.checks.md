# WorkspaceFeatureGuide 视觉检查

脚本退出码只代表 OCR 断言（英文界面；浅色底 OCR 会读歪标题，故用多关键词匹配）。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-auto.png` | 首次进入项目画布后铺满窗口的功能引导：标题、左侧主题导航、右侧说明、底部「Skip Guide / Next」，层级清晰、与背景有明确材质分隔 | 文字被裁切或重叠、按钮无边界、引导只画在项目卡片内、圆角处露出直角底色 |
| `02-next.png` | 切到「Voice Group Control」（2 / 7），导航选中态可辨，「Set up」徽标留在行内 | 内容未更新、选中态缺失、徽标溢出选中行 |
| `02b-cheat-sheet.png` | 第 7 页「Shortcut Cheat Sheet」（7 / 7），底部主按钮为「Get Started」且无播放按钮；Workspaces / Projects / Create cards / Dispatch ideas 四组各有描边卡片，键位标签为圆角小标签并与说明同行右对齐 | 文字被裁切或重叠、键位标签溢出卡片、分组缺失、仍显示播放/暂停按钮 |
| `03-closed.png` | 引导消失，画布与 Dock 完整可用 | 残留遮罩或半透明层 |
| `03b-placeholder.png` | Dock 输入框占位提示「Talk to Norma, / for commands, /help」完整显示 | 提示被截断、与快捷键徽标重叠 |
| `04-help-suggestion.png` | Dock 上方候选「Help / Open the feature guide and shortcuts / /help」，行高、图标、说明与其他候选一致 | 候选缺失、图标或文字错位 |
| `theme-<id>.png`（六个主题） | `/help` 重开后恢复到上次的 7 / 7；主按钮文字清晰（浅色主题为白字深色底）；面板边框、顶部与底部分隔线可见；指令输入已清空 | 黑字压中蓝底、浅色下分隔线或边框缺失、版式与首次不同 |
| `theme-<id>-orchestration.png`（六个主题） | 第 1 页：安装状态行有描边容器、提示条、「Open Orchestration Settings」主按钮与「Why…」链接对比度足够；底部 Skip / Next 完整 | 行被裁切、按钮文字不可读、描边缺失 |

六个主题：ink-black、graphite、warm-obsidian、sapphire-night、moonstone、deep-petrol（全部深色）。「Install failed」红行来自隔离环境未安装 CLI，不算缺陷。不确定时看同一次运行的 `app.log` 和 `report.json`。
