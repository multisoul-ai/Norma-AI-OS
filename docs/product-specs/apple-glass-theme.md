# Apple Glass Theme SPEC

## 1. 背景与目标

Multisoul Desktop 当前使用一套深色纯色 GPUI 主题。目标是将默认视觉升级为苹果风格的玻璃质感主题，让无限画布和 Terminal Live Node 更接近 macOS / Liquid Glass 的清透、层次、轻量材质感。

本次主题改造服务于开发工作台体验：视觉要更精致，但不能牺牲终端可读性、交互稳定性和域层纯净性。

## 2. 范围

### 2.1 In Scope

- 新增双主题 token：`glass_light()` 与 `glass_dark()`。
- 第一版默认固定使用 `glass_light()`。
- 启用窗口级毛玻璃：`WindowBackgroundAppearance::Blurred`。
- 使用透明标题栏，让窗口 chrome 融入内容区。
- 将画布背景改为浅色雾面渐变。
- 将节点外壳改成玻璃面板效果：半透明填充、细边框、顶部高光、柔和阴影。
- Terminal 内容区保持深色高不透明背景，优先可读性。
- 选中和聚焦状态统一使用 Apple 蓝系强调色。

### 2.2 Out of Scope

- 不做运行时主题切换菜单或快捷键。
- 不做单个节点真实 backdrop blur。
- 不做自定义 Metal shader、折射、动态光效。
- 不修改 `canvas`、`terminal`、`runtime` 域层。
- 不引入新 UI 框架或升级 GPUI。

## 3. 用户与使用场景

典型用户是 Multisoul Desktop 的开发者或早期使用者，在 macOS 上使用无限画布管理 live nodes。

关键场景：

- 用户启动应用，默认看到浅色玻璃质感画布。
- 用户按 `Cmd+N` 创建 Terminal 节点，节点以玻璃外壳浮在画布上。
- 用户单击节点，边框以 Apple 蓝反馈选中状态。
- 用户双击 Terminal 聚焦，聚焦状态仍使用 Apple 蓝系，但通过更强边框或阴影与普通选中区分。
- 用户长时间阅读终端输出时，内容区保持稳定高对比，不被背景干扰。

## 4. UI/UX 需求

视觉方向：

- 浅色玻璃为默认：银白、冷灰、轻微雾蓝。
- 整体克制，不做高饱和霓虹、不做大面积彩色渐变。
- 画布像一层明亮磨砂玻璃，而不是纯白页面。
- 节点像浮在画布上的玻璃面板，有轻微深度和边缘高光。
- Terminal 是嵌在玻璃壳里的深色工作区。

交互反馈：

- 选中：Apple 蓝边框。
- 聚焦：同色系更强的边框或阴影，不引入绿色。
- 标题栏：半透明玻璃材质，保留节点拖拽热区。
- 圆角建议从当前 `6px` 调整到 `8px`，保持工具属性，不做过度圆润。

## 5. 技术实现概览

主题层：

- 扩展 `ui::Theme`，从纯 `0xRRGGBB` 色表升级为玻璃材质 token。
- 颜色建议使用 `0xRRGGBBAA`，通过 `gpui::rgba(...)` 渲染。
- 保留 `Theme::dark()` 兼容入口，可指向 `glass_dark()`。
- `Theme::default()` 返回 `Theme::glass_light()`。

窗口层：

- 在 `crates/app/src/main.rs` 的 `WindowOptions` 中设置：
  - `window_background: WindowBackgroundAppearance::Blurred`
  - `titlebar: Some(TitlebarOptions { appears_transparent: true, ... })`

渲染层：

- 在 canvas root 使用 `gpui::linear_gradient(...)` 做雾面浅色背景。
- 节点使用半透明 `node_fill`，配合 `BoxShadow` 和细边框。
- 节点内部增加一个顶部高光层，模拟玻璃反射。
- Terminal 渲染从 `rgb(...)` 改为 `rgba(...)` 或 Theme helper。
- Terminal 内容背景保持接近不透明，避免玻璃效果影响阅读。

## 6. 状态、错误与边界情况

- 如果平台不支持 blurred window，UI 仍需正常显示；半透明、渐变、阴影提供 fallback 玻璃感。
- 如果窗口背景透出复杂壁纸，Terminal 内容区仍必须可读。
- 节点在缩放时标题栏高度、边框和文本不能出现明显错位。
- 退出态 `[process exited]` 占位仍需保持高对比。
- 主题改动不能影响节点 hit test、拖拽、pan、zoom、Terminal 输入路由。

## 7. 非功能性需求

- 不引入新依赖。
- 不升级 `gpui = 0.2.2`。
- 不增加域层对 GPUI 的依赖。
- 主题渲染应保持轻量，避免 per-frame 动态材质计算。
- 改动后 `cargo check -p app` 必须通过。
- 改动后 `cargo test -p ui` 必须通过。

## 8. 风险与权衡

- GPUI 支持窗口级 blur，但不等同于 CSS `backdrop-filter`；节点级玻璃需要用分层视觉模拟。
- 默认浅色主题会比当前深色主题更“苹果”，但 Terminal 可读性风险更高，因此 Terminal 内容区必须保持深色高不透明。
- 双主题先提供 API，不提供切换入口，降低第一版实现面。
- 透明标题栏会更现代，但可能需要后续微调顶部安全区域和交通灯位置。

## 9. 验收标准

- 启动应用后默认显示浅色玻璃主题。
- 窗口背景启用毛玻璃，并使用透明标题栏。
- 画布是浅色雾面渐变，而不是纯白或纯深灰。
- Terminal 节点外壳有玻璃质感：半透明、细边框、高光、阴影。
- Terminal 内容区保持深色且文本清晰。
- 选中和聚焦状态使用 Apple 蓝系，状态可区分。
- `Theme::glass_light()`、`Theme::glass_dark()`、`Theme::default()` 有单元测试覆盖。
- 测试断言符合项目规范：具体正负断言、每个断言带失败说明、测试注释完整。
- `cargo test -p ui` 通过。
- `cargo check -p app` 通过。

## 10. 建议实现路径

- 先扩展 `crates/ui/src/theme.rs`，定义玻璃主题 token 与测试。
- 再调整 `crates/app/src/main.rs`，启用窗口级毛玻璃和透明标题栏。
- 最后调整 `crates/app/src/canvas_view.rs` 与 `crates/app/src/terminal_view.rs`，应用新的材质 token。
