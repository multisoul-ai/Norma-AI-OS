# 工作空间壁纸：Material 页面与范围持久化

## 页面设计

Settings → Wallpaper 按“范围 → 当前预览 → 素材库”排列。英文菜单保留 Workspace / Project；目标名称与范围选择器同一行，打开项目时默认 Project，工作空间总览默认 Workspace，没有展开项目时禁用 Project。

设计依据为 Google Material 的 [自适应布局](https://m3.material.io/foundations/layout/layout-overview/adaptive-design)、[分段选择](https://m3.material.io/components/segmented-buttons/guidelines)、[开关](https://m3.material.io/components/switch/guidelines)、[卡片](https://m3.material.io/components/cards/overview)和[交互状态](https://m3.material.io/foundations/interaction/states/state-layers)。`.agents/skills/apple-design` 的直接操作、即时反馈与键盘习惯用于 macOS 交互；视觉层级采用本次明确要求的 Material。

| 元素 | 实现与目的 |
|---|---|
| 页面标题 | 精简标题，不重复介绍范围与操作 |
| 范围选择器 | 连接式按钮组；选中勾选与键盘焦点分开呈现 |
| 当前壁纸 | 单张预览卡显示名称、类型、作用范围及保存状态 |
| 项目跟随 | `Use Workspace Wallpaper` 左侧文字、右侧开关；整行可点击并支持 Enter/Space |
| 素材库 | Built-in / Your wallpapers 分组、统一 16:9 静态缩略图，动态素材显示 Dynamic 标签 |
| 选择反馈 | 蓝色边框与勾选立即更新；后台保存失败恢复原选择并显示错误 |
| 导入 | `Add wallpaper` 打开原生文件选择器；文件复制与解析在后台进行 |
| 删除 | 窗口对话框展示受影响空间和项目；位置独立于素材库滚动，Escape/Cancel 取消，必须明确激活删除按钮 |

页面内容最大宽度 856dp，素材间距 12dp，单元最小宽度 128dp，最多 4 列；内容宽度低于 620dp 时当前预览纵向排列。按钮、缩略图和跟随行覆盖 hover、pressed、focus、selected 与适用的 disabled 状态。控件优先复用钉死的 gpui-component，补充其当前版本缺失的键盘激活行为。

## 数据范围与读取

素材库全局共用，壁纸选择分开保存：

- `desktop_workspaces.wallpaper_json` 保存工作空间选择。
- `desktop_project_areas.wallpaper_json` 保存该空间中的项目覆盖，空值表示跟随。
- 选择记录使用 `WallpaperReference`，仅包含内置资源标识或自定义文件名。

有效项目覆盖 → 有效工作空间选择 → 内置 Default。资源缺失时显示回退背景与提示，保留原选择记录。空间切换、项目展开/收起及重启都重新绑定范围；同一项目在不同空间中的覆盖互相隔离。

新建空间在同一事务内复制当前空间的壁纸，不复制项目覆盖。没有当前空间时使用原全局选择。旧版本升级将全局选择一次性复制给已有空间，迁移标记和字段建立同事务提交，重复执行不覆盖后续设置。

目录页与旧独立项目窗口维持原背景策略。

## 写入与后台边界

选择立即预览，保存请求捕获 Workspace ID / Area ID 与原选择，通过宿主串行后台写入。返回结果更新原目标，范围切换后的背景不会被迟到结果覆盖；revision 防止旧读取覆盖新预览。

导入开始时同样捕获目标身份；原生选择器关闭后，复制文件及保存由后台执行。删除先查询引用范围并确认，再在后台重置数据库引用、删除共享文件；空间引用改为 Default，项目引用清除后跟随空间。

动态背景继续使用后台解码与低拷贝 `surface()`，切换选择更新播放源。素材库及当前卡仅使用后台生成的静态预览缓存。

## 验收映射

| 行为 | 自动验证 |
|---|---|
| A/B 空间及同项目两处覆盖隔离 | `crates/storage/tests/desktop_wallpaper.rs` |
| 跟随、清除覆盖、新建复制、升级幂等与重启 | storage 壁纸集成测试、app desktop/scoped 测试 |
| 缺失回退、删除引用恢复 | `wallpaper_scope_tests.rs`、storage 壁纸集成测试 |
| 快速切换、保存失败回滚 | `wallpaper_actions_tests.rs`：后台迟到保存与 SQLite 触发器故障注入 |
| 响应式网格与预览布局 | `wallpaper_layout_tests.rs`；新增测试先失败后实现 |
| 按钮键盘激活与禁用 | `wallpaper_keyboard_tests.rs`；每种输入只执行一次 |
| 屏外删除确认 | `wallpaper_actions_tests.rs` WP-10；先验证旧实现失败，再验证窗口对话框及取消保留引用 |
| 存储与动态背景边界 | storage / performance / video-wallpaper 护栏 |

## 真实 App 记录

通过 Computer Use 操作隔离的 `Norma Wallpaper QA.app`，数据目录 `/tmp/norma-wallpaper-qa-20261004/data`。

已验证 Alpha/Beta 空间壁纸及同一项目覆盖互相隔离、跟随切换、退出重启恢复；新建 Gamma 使用 Alpha 空间壁纸且没有项目覆盖。原生导入测试 WebP、静态图片、GIF 和 MP4 切换均显示正确背景。浅色 Paper Blue 与深色 Sapphire Night 页面已检查，最终跟随行的鼠标、Tab、Enter 和 Space 操作均正常。

自动测试覆盖实际删除与引用清理；真实 UI 已从素材库底部打开居中删除对话框，确认显示 `Alpha / Shared Project` 及回退规则。Cancel 和 Escape 均关闭对话框并保留文件、项目覆盖；对话框初始焦点按 Enter 不执行删除。真实 UI 不执行永久删除。动态切换检查画面结果与性能护栏，未额外测量解码线程退出时延。

截图位于 `/tmp/norma-wallpaper-qa-20261004/`：`wallpaper-project-light.png`、`wallpaper-project-dark.png`、`wallpaper-workspace-dark.png`、`wallpaper-project-light-following.png`、`wallpaper-project-dark-following.png`、`wallpaper-delete-confirmation.png`。浅色/深色项目页面及删除对话框均直接保存对应观察返回的完整窗口图像。

本次 Material 页面聚焦测试 23 项通过，`cargo check -p app` 与最终 `cargo build -p app --release` 通过。存储、性能和视频壁纸护栏通过。全仓文件行数检查仍被并行手机模块 `crates/app/src/mobile/server/connection.rs` 的 515 行阻断；壁纸相关新增/修改文件均低于 500 行。
