# canvas-presentation-layout

- `01-before-present.png`：1440×840，六张 Agent 和两张终端正文完整可见；首卡靠近画布左侧，无细条。
- `02-present-5.png`：第 5 轮网页/文件展示完成，右抽屉显示文件；卡片仍按可见集合排布，无隐藏卡片空档。
- `03-independent-preview.png`：独立预览副本可见；保留两个文件标签；画布布局正常。
- `04-manual-arrange.png`：手动重排后卡片连续占位。
- `05-after-restart.png`：重启后布局和已选内容恢复。

硬断言：真实 CLI 返回 content_drawer；首轮 Created、后续 Reused 且表面 ID 不变；--new 返回不同 ID；六个 Agent 正文标记和两个独立终端正文标记可见；可见卡片分布覆盖多行和左侧。

- `02-browser-content.png`：回环网页正文真实显示，画布卡片仍正常排布。
- `06-legacy-master-recovered.png`：注入旧隐藏 Master 的 62px 坏布局后，真实项目恢复无细条与隐藏首格空档。
- `01-visible-master.png`：第一张终端占左侧主区域，六张 Agent 和另一张终端 detail 在右侧；标题与正文可读，无隐藏占位。
