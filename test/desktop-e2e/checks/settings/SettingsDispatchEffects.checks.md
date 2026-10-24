# 设置 · 特效

参考的 skill 与文档：`apple-design`（内容优先、直接反馈、可访问性）、`docs/design-docs/material-menus.md`（来源菜单的 48px 选项、勾选与键盘浏览）。结构与壁纸页一致：范围 → 当前特效 → 特效库 → 按来源 → 播放。

| 截图 | 要看到 | 不应看到 |
| --- | --- | --- |
| `01-effects-page-dark.png` | 侧栏“通用”组里选中的 Effects（带图标）；页头标题与说明；Personalize + 工作空间名；Workspace / Project 分段按钮（Workspace 带勾，Project 灰掉）；当前特效卡里有迷你场景预览 | 空白页面、Project 可点、预览里只有背景没有指令栏 |
| `02-fireworks-picked.png` | 当前特效名为 Fireworks，说明文字是烟花的描述；Replay 按钮可见；预览里有金色火星 | 仍显示 Meteor、说明文字与名称不一致 |
| `03-source-menu-open.png` | Voice dispatch 的下拉菜单展开：每项 48px，名称 + 一行说明，当前值（Follow current effect）右侧有勾且行高亮；其余行无勾；菜单不被窗口裁切 | 说明里出现 `{style}` 之类未替换的占位符、两行同时有勾、菜单文字重叠 |
| `04-playback.png` | Playback 分组只保留速度三档（Relaxed 被选中） | 三档同时选中、仍出现连续派发简化或减少动态效果的降级开关 |
| `05-effects-page-light.png` | Paper Blue 下整页可读：选中的卡片有蓝色描边与右下角勾标、标签变蓝；其余卡片无勾；状态行不停留在 “Saving…” | 白底上看不清的文字或缩略图、两张卡片同时被选中、状态行一直是 “Saving…” |
