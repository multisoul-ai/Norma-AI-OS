# 三个浅色主题的材质修订与终端对照

状态：已被用户取消。现行方向为仅保留深色，见 `docs/design-docs/2026-10-27-dark-theme-collection.md`。以下保留为历史验收记录，不再等待浅色 A/B 选择。

保留现有布局和六个主题。修改 Paper Blue（冷纸白与钴蓝）、Linen Sage（暖纸白与鼠尾草绿）、Silver Violet（银白与柔紫）；深色主题保留原色。

## 材质与语义色

壁纸先叠加所选浅色主题的提亮层；左右抽屉的模糊壁纸样本使用同一提亮层，再叠加 179/255（约70%）主蒙层。70% 指该蒙层，不是整个控件或最终合成层的不透明度。文字不透明度独立。

卡片外框和标题采用稳定浅色阅读面，正文、辅助文字为深色实色。Dock、指令候选和语音按钮同步使用与底色相容的前景色。Apple 的内容优先与材质层级落实为：壁纸保留色彩但退居背景、标题边界完整、强调色用于当前状态，避免多层低透明度造成灰脏。

A 版：浅色终端阅读面，16 色 ANSI 改用足够深的颜色；程序自定义 RGB 与 256 色 cube 保持原值。

B 版：相同浅色标题和外框，搭配深色终端阅读面；原深色 ANSI 不变。仅构建对照，不新增主题或设置。

## 验收映射

| 标准 | 证据 |
| --- | --- |
| 浅色标题、辅助字、终端阅读面稳定 | ui light_theme_material_tests |
| ANSI 16 色可读，RGB/256 色保留 | app cards/term/light_palette_tests |
| 左右70%主蒙层一致 | desktop_drawer_material_tests |
| 主题变化传播到宿主和项目 | desktop_surface_theme_tests、desktop_theme_tests |
| 首页、项目、双抽屉一致 | settings-theme-consistency --visual |
| focus/selected/hover/pressed/disabled | 同场景逐帧检查并保存截图 |
| 视频仍走低拷贝路径，无热路径IO | video/performance/storage 护栏 |

三主题×两种终端方案使用相同真实项目、两个 PTY 输出和窗口尺寸对照。HTML 如用于汇总，仅展示真实 App 截图，不能替代验收。用户已要求不用 Computer Use，环境失败应保留 ENV 证据并报告。

## 2026-10-27 验证结果

A 三主题连续验收通过21次看图；B前两主题14次看图通过后等待响应超时，原轮保持ENV失败，银灰紫定向补跑7次看图通过。ui 15项与app主题聚焦23项通过，cargo check、GPUI release和相关护栏通过。app全lib最后一次为1621通过、1失败、9忽略，唯一对比失败已修复并通过后续聚焦测试，未重跑全lib。

真实截图对照：`artifacts/light-theme-comparison/index.html`；逐项命令、二进制SHA、历史失败和限制：`artifacts/light-theme-comparison/verification.md`。

六主题补充自动流程在 `artifacts/desktop-workspace-e2e/2026-10-27T17-43-33Z/report.json` 通过（170.51秒），包含浅深切换后文件编辑与磁盘保存断言。深色首页、双抽屉、外框与输入焦点已看图确认；深色分段 hover/pressed 的独立视觉差异仍不确定，不能宣称全部交互态已完成验收。详细结论见同目录 `visual-review.md`。
