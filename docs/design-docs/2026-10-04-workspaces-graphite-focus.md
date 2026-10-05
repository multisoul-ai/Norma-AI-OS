# Workspaces 首页：深色专注

保留现有首页布局，按用户选定的第三张 C「深色专注」优化材质、对比和操作反馈。

## 设计落地

- `apple-design`：以内容优先和清晰层级为基线；名称、项目数量与空态提示保持可读，主操作使用统一蓝色。
- `emil-design-eng`：降低装饰色和壁纸干扰；卡片采用中性石墨材质、细边框及 14px 圆角。
- `gpui-styling` / `gpui-elements`：首页局部派生主题，复用现有 gpui-component 按钮；覆盖悬停、按下、键盘焦点与禁用反馈。
- 卡片不透明度为 240/255（约 94%）；深色壁纸遮罩为 122/255（约 48%）。采用压暗处理，没有新增背景模糊或 CPU 视频帧处理。
- 浅色主题保留深色主文字；布局、工作空间数据模型与全局主题不变。

## 验收对应

| 标准 | 证据 |
|---|---|
| 原布局与石墨色卡片、实心蓝色主按钮 | 真实 App `home.png` |
| 6 个主题、白/黑壁纸上的辅助文字及蓝色按钮文字达到 4.5:1 | `desktop_home_card_text_remains_readable_over_wallpaper` |
| 静止、悬停、按下和焦点颜色可区分 | `desktop_home_material_has_distinct_interaction_states`；真实 App `hover.png` |
| 搜索过滤和清空恢复 | 真实 App 输入 `norma-os` 后显示 1 of 5，清空恢复 5；`search.png` |
| 卡片菜单不误触导航 | 真实 App 打开 Rename/Delete 菜单，仍在首页；`menu.png` |
| 空工作空间打开及返回 | 真实 App 打开 `norma-os` 显示空画布，通过已注册 Control–Command–Up 返回；`open-empty.png` |
| 创建入口和取消 | 真实 App 打开 New Workspace，空名称时创建禁用，取消后仍有 5 个工作空间；`create-dialog.png` |

真实 App 使用独立 `Norma Graphite Preview.app` 和数据库副本验收。截图位于 `artifacts/workspaces-graphite/`，最终首页截图为 `home.png`。

## 验证

- `rtk cargo test -p app desktop_home --lib`：4 项通过。
- `rtk cargo test -p app desktop_workspace_card --lib`：2 项通过。
- `rtk cargo check -p app`、`rtk cargo build -p app --release`：通过。
- 本次 4 个 Rust 文件的 rustfmt 检查、禁止 allow 检查、视频壁纸性能护栏：通过。
- 最终全仓 `check-max-file-lines.sh` 与本次修改的 `git diff --check`：通过。
- 未使用已禁用的 `crates/e2e`。

## 顶部样式延伸到项目与 Agent 画布

- 继续应用 `apple-design`、`emil-design-eng`、`gpui-styling` / `gpui-elements`：沿用首页石墨材质、52px 高度、细分隔线、系统字体与蓝色主操作；固定顶部位置帮助识别当前层级。
- 项目总览居中显示「工作空间 / All projects」；展开项目后显示「工作空间 / 项目名称」。名称跟随 `expanded_area_id`，不采用可能停留在旧项目上的缓存名称。
- 两侧控件预留对称宽度，名称真正对齐窗口中心；窄窗收起品牌和保存状态，将 Add Project 收为图标，并截断长名称，避免覆盖控件。
- gpui-component 按钮提供即时 hover、pressed、focus 与 disabled 反馈；添加项目和返回工作空间使用已有导航路径。
- 顶部栏由窗口宿主绘制，嵌入预览不重复绘制；目录缩放时由现有页面呈现接管。没有新增动效或视频处理路径。
- 项目网格保持原有顺序、列数与 16:9 比例；顶部最小间距改为 52+12=64px，让卡片避开顶部栏，镜头目标同步使用相同布局。

验收映射：`desktop_top_bar_tracks_workspace_and_project_at_window_center` 覆盖总览、alpha/beta 切换、窄窗居中、顶部避让与返回目录；现有 layout/camera/reflow 测试覆盖新顶部安全间距与镜头位置。真实 App 的总览、画布、切换、添加项目取消和返回路径另存截图。

聚焦验证：`rtk cargo test -p app desktop_ --lib` 55 项通过；`rtk cargo test -p app chrome --lib` 36 项通过；`rtk cargo check -p app`、`rtk cargo build -p app --release`、本次文件 rustfmt、diff 空白检查、行数/禁止 allow/视频性能护栏通过。并行桌面测试首轮曾在已有异步 Agent 创建用例失败，隔离重跑及完整桌面重跑均通过。

真实 App 使用独立 `Norma Graphite Preview.app` 与隔离数据库，通过 Computer Use 完成以下验收。`norma-os` 工作空间内的 `Agent Canvas`、`Product UI` 为隔离目录中的空测试项目，未启动 Agent 或 PTY。

| 流程 | 实际结果与截图 |
|---|---|
| 打开项目总览 | 中间显示 `norma-os / All projects`，卡片标题避开顶部栏；`project-overview.png` |
| 展开 Agent Canvas | 中间显示 `norma-os / Agent Canvas`；`agent-canvas.png` |
| Dock 切换到项目 02 | 中间同步更新为 `norma-os / Product UI`；`project-switch.png` |
| 返回项目总览 | Option–Command–Up 后恢复 `All projects`；`project-overview.png` |
| 添加项目并取消 | Add Project 打开真实弹窗，Cancel 关闭且项目数量不变；`add-project-dialog.png` |
| 返回工作空间首页 | 顶部返回按钮恢复原首页样式，仍显示 5 个工作空间 |

上述截图均保存在 `artifacts/workspaces-graphite/`。预览 App 最终停留在工作空间首页；正式安装 App 与正式数据库未替换。

## 顶部透明度调整

- 继续参考 `apple-design` 的材质层级与文字可读性、`emil-design-eng` 的克制视觉和 `gpui-styling` 的背景合成方式。顶部只降低背景 alpha，文字和主操作保持独立绘制。
- 首页、项目总览、Agent 画布共用 `workspace_chrome_theme`：深色顶部为 184/255≈72% 不透明（约 28% 透明），浅色顶部为 214/255≈84% 不透明（约 16% 透明）。相比之前 245/255≈96% 不透明，壁纸能更明显透出。
- 提亮深色顶部的工作空间名称、保存状态与层级分隔符；浅色顶部保留深色文字。沿用 52px 高度、居中标题和现有按钮反馈。
- 使用 GPUI 背景 alpha 合成，未引入额外模糊、视频帧复制或动效。卡片继续使用原材质。
- `desktop_top_bar_translucent_text_remains_readable_over_wallpaper` 验证 6 个主题、纯白/纯黑壁纸下三级文字均达到 4.5:1，并限制材质保持可见透明度；聚焦顶部测试 2 项、首页测试 4 项及 `cargo check -p app` 通过。

本轮 `cargo build -p app --release`、目标文件 rustfmt/diff 检查、行数/禁止 allow/视频性能护栏通过。使用同一隔离预览 App 的新二进制，通过 Computer Use 确认：首页顶部透出壁纸，项目总览显示 `norma-os / All projects`，画布显示 `norma-os / Agent Canvas`，Dock 切换到项目 02 后同步显示 `Product UI`，顶部返回按钮恢复 5 个工作空间的首页。截图为 `transparent-home.png`、`transparent-project-overview.png`、`transparent-agent-canvas.png`，均位于 `artifacts/workspaces-graphite/`。
