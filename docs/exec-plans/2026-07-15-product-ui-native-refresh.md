# Product UI 原生改造实施计划

## 目标

把源工作区中的 `docs/product-ui.html` 视觉语言移植到 Norma OS 的 GPUI 原生界面，覆盖 Launchpad、Canvas、Settings 三个核心页面。改造只消费现有 app/runtime 状态，不复制业务状态，不改背景资源选择、视频壁纸低拷贝链路、节点创建/焦点/输入路由或自动布局算法。

## 参考基线

- 视觉参考：源工作区 `docs/product-ui.html` 及 `docs/design-docs/core-ui-v2/{launchpad,canvas,settings}.png`。
- 设计 token：`docs/design.md` §3–§9。
- 当前业务 seam：`MultisoulApp` 的既有 action、`Runtime`、`MasterLayoutState`、`WallpaperSettingsState` 与 Voice 状态机。

## 不变边界

1. `render_wallpaper_layer` 的 Static/Gif/Video 分支、source-derived render id 与 macOS `surface()` 视频路径保持不变。
2. `apply_workspace_reflow_for_area`、master/detail 切换、safe area、节点 bounds 和 resize 派发保持不变。
3. Terminal/Agent/Browser/Preview 的创建、关闭、选择、焦点、滚轮和输入路由保持原 action/handler。
4. Settings 的即时生效、异步持久化和 debug selector 保持可用。
5. Launchpad 继续使用固定 `wallpapers/home.mp4`，Canvas wallpaper 设置不得影响它。

## 视觉落点

1. `crates/ui/src/theme.rs`
   - 迁移到 Warm White / Graphite / Cobalt / Violet / Acid / Coral / Mint 品牌色。
   - 浅色正文三层、光学玻璃卡片、深色 Terminal content well。

2. Launchpad
   - 轻色雾面背景叠加现有视频层。
   - 宽窗口采用左侧项目入口/最近项目、右侧工作区预览；紧凑窗口隐藏预览并保持现有入口可用。
   - 最近项目使用 64px 行与可滚动容器，保留全部记录。

3. Canvas
   - 现有壁纸保持 100% 覆盖；移除无语义装饰轨道。
   - 卡片使用 22px 光学玻璃外壳、44px 标题栏、类型/身份/状态分层。
   - 顶部工具栏迁移为底部 52px Command Dock；Voice 状态嵌入 Dock。
   - 新增只读 minimap/zoom 指示，数据来自当前 document/viewport，不写入布局。

4. Settings
   - 24px 浅色玻璃窗口、稳定侧栏、分组列表与 header action。
   - 保留所有现有 section、selector 和 click handler。

## TDD 竖切

1. Theme：先更新公开 token 测试为设计规范字面量，确认旧值导致红灯，再迁移实现。
2. Launchpad：先把“最近项目全部保留”和宽/紧凑双栏布局写成测试，再实现列表与预览。
3. Canvas chrome：先增加 Dock 底部位置、Voice/Settings selector 与节点标题栏尺寸测试，再改渲染。
4. Settings：先增加浅色窗口 token/布局和 header action 测试，再改导航、header、分组行。
5. E2E：通过真实渲染树点击 Dock → Settings、创建两个 Terminal → 验证 reflow，确认 UI 改造没有断开公开交互。

## 验收标准与自动化证据

| 验收标准 | 自动化证据 |
| --- | --- |
| 三页视觉语言来自原型 | `cargo test -p ui` + app UI contract tests + 最终真实截图 |
| 不改变业务流程 | `cargo test -p e2e`，尤其 Terminal、Settings、Browser、Voice 用例 |
| 背景功能不受影响 | wallpaper 单测、`e2e_11_launchpad_video`、`check-video-wallpaper-performance.py` |
| 自动布局不受影响 | `e2e_01_create_terminal_focuses_and_reflows`、master layout 单测 |
| GPUI/应用可构建 | `cargo check -p app`、`cargo build -p app --release` |
| 无结构/性能回退 | storage/performance/max-lines/no-allow/guide lint 与依赖边界检查 |
| 与原型一致 | 真实 `.app` 1440×900 截图，对照 Canvas/Launchpad/Settings 参考图逐项复核 |

## 完成顺序

1. Theme 与共享尺寸。
2. Launchpad。
3. Canvas card chrome、Dock、minimap。
4. Settings。
5. 聚焦测试 → e2e → workspace tests → release build → L3 截图。
6. 按截图差异迭代，直到结构、层级、色彩与交互入口一致。
