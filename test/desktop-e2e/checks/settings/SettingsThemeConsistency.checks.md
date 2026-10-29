# 六套深色主题同步验收

每个主题都检查以下真实 App 截图，不能以 OCR/像素断言代替看图。

- `*-home.png`：首页保持原布局，工具栏、卡片与标题可读，背景色系跟随选择。
- `*-drawers.png` / `*-project.png`：左右抽屉及画布外框同主题；三级文字、边框清晰；壁纸和项目内容完整。
- `*-overview-selected.png`：运行概览、空态、想法输入可读，想法分段选中状态可辨，空输入发送按钮呈禁用状态。
- `*-tab-hover.png` / `*-tab-pressed.png`：已指派分段悬停与按下均可辨，文字不消失。
- `*-input-focus.png`：右栏地址输入文字正确、焦点清晰，未发出请求。

默认墨黑：Ink Black；另外五套：Graphite / Warm Obsidian / Sapphire Night / Moonstone / Deep Petrol。

每项标记符合、不符合或不确定，并注明实际证据目录和未通过项。

## 深色系列模型验收

`--visual` 默认遍历六套深色主题，逐项写入实际看图结论。

- 首页、项目、双抽屉截图保存在 `*-home.png`、`*-project.png`、`*-drawers.png`。
- `*-input-focus.png`、`*-selected.png`、`*-hover.png`、`*-pressed.png` 覆盖真实输入状态。
- 同一夹具、相同 1440×840 窗口、左右抽屉通过真实拖拽各调整为300pt、两个真实 PTY，输出 DARKONE/DARKTWO 与红绿 ANSI。
- 墨黑采用中性黑灰与银灰焦点，石墨灰层次稍亮，暖曜黑为暖黑与香槟金；蓝宝石保留配色，月岩和深海石替换旧紫、绿。
- Apple 设计原则落实为稳定阅读面、克制背景、清晰内容层级与即时反馈；不更改布局。
- 主抽屉蒙层为 153/255（约 60% 不透明度）。玻璃材质会受到壁纸色彩影响，不能把合成颜色当成纯色 token；旧 70% 截图仅用于前后对照。
- 只对实际看过的状态声明符合，不因自动步骤通过就推定按下帧正确。

可用 `NORMA_THEME_REVIEW_THEME` 指定目录内一个稳定主题 id；未知值报 ENV，不静默跳过。单主题补跑不证明跨主题更新传播。`mineral` 会在同一进程依次验证月岩和深海石。

新增 `*-settings.png` 检查六主题及深色选中勾；`*-selected-hover.png` / `*-selected-pressed.png` 检查已经选中的分段仍有反馈；`*-agent-status.png` 用假 CLI 经真实 Hook 生成有内容的运行/审批状态，不能用空列表替代。
