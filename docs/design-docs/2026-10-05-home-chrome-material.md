# 工作空间首页材质修复验收

日期：2026-10-05。

## 设计与实现

参考 `.agents/skills/apple-design/SKILL.md`、`.agents/skills/emil-design-eng/SKILL.md` 与 `/Users/alan/.codex/skills/gpui-styling/SKILL.md`。

- 一致性与平台熟悉性：首页标题、分组标签、工作空间卡片直接复用 Dock 材质色与描边，文字沿用顶部栏的三级白色前景；浅色主题也不再出现大块白底。
- 信息层级与可访问性：保留现有标题、数量、预览和操作布局，预览内层仅使用 14% 同色底；六个主题在黑白壁纸上的文字对比由聚焦测试验证，最低要求为 4.5:1。
- 即时反馈：保留卡片 hover、pressed 与 focus，次级按钮复用 gpui-component，使用透明底与可见的浅色交互色。
- 性能取舍：本次仅调整 GPUI 配色，不增加动效、模糊或壁纸帧处理；无需减少动态效果的新增分支。

实现位置：`crates/app/src/workspace/desktop/home.rs` 与 `home_card.rs`；聚焦测试位于 `crates/app/tests/unit/desktop_home_tests.rs` 与 `desktop_home_material_tests.rs`。

## 验证

先更新材质验收测试，确认旧实现因 PaperBlue 卡片材质不同于 Dock 失败，再修改实现。

- `rtk cargo test -p app desktop_home --lib`：5 项通过。
- `rtk cargo check -p app`、`rtk cargo build -p app --release`：通过。
- 修改文件的 rustfmt 与 diff 空白检查：通过。
- 文件行数、禁止 allow、视频壁纸性能护栏：通过。

## 真实 App 验收

使用 Computer Use 操作由本次 release 构建生成的隔离 App：`/private/tmp/norma-home-material-fix/Norma Home Material Preview.app`。数据使用独立测试目录，壁纸采用用户截图中的 `1331865.webp`。

真实切换 PaperBlue 与 SapphireNight，检查含项目和无项目的工作空间卡片、标题、分组、数量、省略号及重命名/删除菜单；白底已消除，文字与交互反馈清晰。打开工作空间后通过快捷键返回首页，最终保留 PaperBlue 预览。

截图证据：

- 浅色首页：`artifacts/home-material-fix/light.png`
- 深色首页：`artifacts/home-material-fix/dark.png`
- 浅色菜单：`artifacts/home-material-fix/menu-light.png`
- 深色菜单：`artifacts/home-material-fix/menu-dark.png`

本次验收使用隔离预览，正式安装的 App 未替换。
