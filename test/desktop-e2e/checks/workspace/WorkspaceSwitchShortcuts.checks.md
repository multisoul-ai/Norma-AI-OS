# WorkspaceSwitchShortcuts 视觉检查

脚本退出码只代表 OCR 和 SQLite 断言。下面每一张由 Agent 打开图片后单独判断，结论写「符合 / 不符合 / 不确定」。`light-` 与 `dark-` 两组各检查一遍；符号键帽（⌘⇧↑）OCR 不稳定，只能看图。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `<主题>-01-directory.png` | 全部工作空间目录（有「Continue working」），Alpha、Beta 两张卡 | 切换器画面、引导弹层 |
| `<主题>-02-switcher.png` | ⇧⌘↑ 之后的工作空间切换器（空间卡内有「A canvas for your projects」） | 仍停留在目录页 |
| `<主题>-03-all-workspaces.png` | ⇧⌘K 之后回到目录页 | 仍停留在切换器 |
| `<主题>-04-dock-tooltip.png` | Dock 右侧切换按钮的悬浮提示为「Switch Workspace」，键帽为 ⌘⇧↑，文字在当前主题下清晰可读 | 提示以 K 结尾（旧 ⌘⇧K）、提示缺失或被截断 |
| `<主题>-05-cheat-sheet.png` | 速查表「Workspaces」分组里切换工作空间为 ⇧⌘↑、全部工作空间为 ⇧⌘K | 两行键位仍是旧值（互换前），或两行显示相同键位 |
