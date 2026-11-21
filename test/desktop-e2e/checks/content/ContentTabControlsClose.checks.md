# content-tab-controls-close 看图清单

运行 `scripts/desktop-e2e.sh content-tab-controls-close --visual`。使用英文蓝宝石之夜。逐项打开原始 PNG，按画面判断。

| 证据 | 要看到 | 不应看到 |
| --- | --- | --- |
| `00a-middle-drag-en.png`、`00b-middle-closed-en.png` | 标签外按中键，移入 Spare.txt 松开时四个标签保留。完整中键点击仅关闭 Spare.txt，First.txt 始终选中，正文仍是 FILE BEFORE。 | 拖入松开时丢失标签，完整点击改变选中项或打开菜单。 |
| `01-search-en.png`、`02-web-before-en.png` | 英文占位提示支持名称、路径或域名。选中勾标存在且独立于焦点。确认后显示回环网页 WEB BEFORE。 | 中文控件文案、确认后浮层遮挡网页。 |
| `03-web-refreshing-en.png`、`04-web-updated-en.png` | 由菜单 Reload 触发。同一回环地址等待响应时，英文刷新提示、细进度条和转圈清楚。完成后同一标签显示 WEB UPDATED。 | 刷新期间显示已完成正文或完成后仍有进度。 |
| `05-last-menu-en.png` | 菜单标题为 Loopback，四项关闭依约排列，分隔线后是 Reload。仅末位的关闭右侧项禁用。 | 网页盖住浮层，行重叠、裁切或禁用态对比不可辨。 |
| `05a-right-click-en.png` | 右键未选中的 Plan.txt 后，菜单标题为 Plan.txt，Loopback 保持选中。 | 右键改变当前标签，或打开了 Loopback 的菜单。 |
| `06-unselected-menu-en.png`、`07-right-closed-en.png` | Shift+F10 所指目标是 Plan.txt，而 First.txt 保持选择。操作后只移除其右侧 Loopback。 | 胶囊焦点与当前选择混淆，关闭了 Plan.txt 或 First.txt。 |
| `08-single-menu-en.png` | 最后一个 Plan.txt 选中，正文 ZULU FILE。关闭其他和关闭右侧禁用，其余可辨。 | 文件内容被错误提示覆盖、其他菜单项也显示禁用。 |
| `09-browser-recovered-en.png`、`10-resumed-en.png` | 通过加号和实际地址恢复网页。关闭其他并重启后只恢复 Plan.txt，内容一致。 | 之前关闭的标签重现。 |
| `10a-draft-cancelled-en.png`、`10b-draft-conflict-en.png`、`10c-draft-kept-en.png` | 取消批量关闭后仍可见原草稿和两个标签。外部改写后点 Save all，确认内显示保存冲突。取消后可再次看到原草稿。 | 标签被部分关闭、外部磁盘版本替换编辑器内容、确认或错误文字裁切。 |
| `11-all-closed-en.png`、`12-empty-restored-en.png`、`13-project-recovered-en.png` | 明确放弃草稿后，全关及重启均为原有首页。点击 Tools 的 Files 可恢复项目文件工作台，头部有标签操作。 | 空白、旧标签复活、首页工具或工作台操作裁切。 |

左右键、中键和键盘经隔离窗口输入通道发送。当前 App 主题目录只有暗色主题，写入旧浅色主题 ID 会迁移为墨黑。
