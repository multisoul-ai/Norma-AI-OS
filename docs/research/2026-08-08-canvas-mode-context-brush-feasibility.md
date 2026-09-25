# Canvas / Live Review 双模式与 Context Brush 可行性研究 Spec

- 日期：2026-08-08
- 状态：可进入分阶段原型；尚未进入实现
- 研究范围：顶部模式 Tab、同一 Runtime 下的双画布模式、任意窗口实时画面、区域框选上下文、多 Agent 任务分发
- 事实基线：当前仓库实现；本文明确区分“已存在能力”和“建议新增能力”

## 1. 结论

方案整体可行，但应把它定义为**同一个 Workspace 的两种呈现模式**，而不是创建第二个 Canvas Document、第二个 Runtime，或新增一个与 `AppScreen::Canvas` 平级的页面。

建议的产品模型是：

```text
Workspace（始终处于 AppScreen::Canvas）
├── Canvas 模式
│   └── 当前无限画布、Terminal、Browser、Preview、Agent Card
└── Live Review 模式
    ├── 中央 Live Surface：Web / Simulator / 桌面 App 窗口
    ├── Context Brush：框选或圈选画面区域
    ├── Norma 对话：指令 + 已选视觉上下文
    └── Agent Crew：多个 Agent 的任务、状态和结果
```

这样做能保留当前 PTY、Agent、任务队列、Canvas viewport 和节点会话。模式切换只改变投影视图和输入路由，不销毁正在执行的工作。

核心可行性判断：

| 能力 | 可行性 | 主要依据 | 主要缺口 |
|---|---:|---|---|
| 顶部 `Canvas / Live Review` Tab | 高 | 已有统一 54px 顶栏和 Workspace Chrome | 需要处理拖拽区与子控件命中、焦点态 |
| 同一 Runtime 无损切换模式 | 高 | Runtime 已独立持有 Document、Focus、LiveNode 与会话 | 需要新增 app 级模式和切换清理 |
| 显示现有 Ego 窗口实时画面 | 高 | 已有 ScreenCaptureKit、420f、IOSurface、`surface()` 低拷贝链路 | 实现目前与 Ego 强绑定 |
| 任意 macOS 窗口实时画面 | 中高 | 现有捕获链路可抽象，ScreenCaptureKit 已可枚举窗口 | 当前没有通用窗口选择器和通用 Capture Store |
| iPhone Simulator 画面 | 中高 | 可先把 Simulator 当作普通 macOS 窗口 | 当前没有 Simulator 专用集成 |
| 矩形区域 → 静态图片上下文 | 中高 | 已有一次性窗口截图、Preview 和图片落盘先例 | 420f 实时帧目前没有通用裁剪/编码接口 |
| 区域上下文发送给多个 Agent | 中 | 当前任务链路可传字符串和文件路径 | 现有 Agent 协议没有 typed image attachment |
| 自由画笔遮罩、区域实时追踪 | 中低 | 几何与 overlay 可实现 | 外部窗口只有像素，暂无语义锚点或元素追踪 |
| 原生多模态图片输入 | 中低 | 底层模型供应商可能支持 | 当前 `NormaSession`、协调与 PTY 分发均为文本 |

推荐先做“**低拷贝实时画面 + 矩形 Snapshot Brush + 图片文件引用 + 多 Agent 共享 Context ID**”。这条路径能验证核心交互，同时不要求一次性改造所有模型 Provider 和 Agent CLI。

## 2. 目标与非目标

### 2.1 目标

1. 用户可在顶部 Tab 即时切换 Canvas 与 Live Review。
2. 切换不会重启或暂停正在运行的 Terminal / Coding Agent。
3. Live Review 中央区域可以显示一个现有节点或外部 macOS 窗口。
4. 用户用画笔框选画面区域，将其作为下一轮对话和 Agent 任务的明确上下文。
5. 多个 Agent 收到同一个上下文标识、截图文件和用户指令；执行状态持续可见。
6. 切回 Canvas 后，原节点位置、viewport、选择状态和会话不被 Live Review 布局覆盖。

### 2.2 首期非目标

- 不把外部窗口变成可远程点击、输入的交互式镜像。
- 不保证框选区域能跟随外部 App 内部控件移动。
- 不在首期实现 DOM、Accessibility Element 或 Figma Layer 级语义选取。
- 不在首期要求所有 Provider 支持原生视觉 token。
- 不把 Live Review 的 Crew 布局写回 Canvas 节点 bounds。
- 不在每一帧执行 CPU RGBA 转换、PNG 编码或 SQLite 写入。

## 3. 当前架构事实

### 3.1 Workspace 页面与渲染入口

当前 `AppScreen` 只有 `Launchpad` 与 `Canvas` 两个状态：[`crates/app/src/app/state.rs`](../../crates/app/src/app/state.rs#L7)。Workspace 的主渲染分支在 [`crates/app/src/app/render.rs`](../../crates/app/src/app/render.rs#L47)，Canvas 分支组合了缓存画布、Working Snake、Workspace Chrome、错误提示和语音区域。

高成本画布被封装为观察 `MultisoulApp` 的 `CanvasSurfaceView`，在独立 Entity 中缓存和重建：[`crates/app/src/app/canvas_surface.rs`](../../crates/app/src/app/canvas_surface.rs#L6)。实际入口 `render_canvas_surface` 继续绘制 Workspace backdrop 与 Canvas：[`crates/app/src/app/render.rs`](../../crates/app/src/app/render.rs#L114)。

`MultisoulApp` 当前只持有一个 `Runtime`，同时持有 Browser、Preview、Ego Preview、交互态、布局态、持久化调度器和 Agent 协调相关状态：[`crates/app/src/app/mod.rs`](../../crates/app/src/app/mod.rs#L153)。这是复用同一工作会话的正确所有权位置。

不建议新增 `AppScreen::LiveReview`。仓库中 Settings、Keyboard、IME、Voice、Agent Actions、Ego Preview、frame loop 与 Norma server 等大量逻辑都用 `AppScreen::Canvas` 作为 Workspace 是否活跃的门控。例如：

- 键盘输入门控：[`crates/app/src/app/keyboard.rs`](../../crates/app/src/app/keyboard.rs#L241)
- IME 提交门控：[`crates/app/src/app/ime.rs`](../../crates/app/src/app/ime.rs#L103)
- Ego 实时预览刷新门控：[`crates/app/src/app/ego_preview.rs`](../../crates/app/src/app/ego_preview.rs#L168)
- Agent action 门控：[`crates/app/src/app/agent_actions.rs`](../../crates/app/src/app/agent_actions.rs#L28)
- frame loop 的 Canvas 活跃标志：[`crates/app/src/app/frame_loop.rs`](../../crates/app/src/app/frame_loop.rs#L277)

若把新模式做成新的 `AppScreen`，这些功能会被错误关闭，或者需要大范围修改既有语义。更安全的边界是：`AppScreen::Canvas` 表示“已进入 Workspace”，再新增一个 app 级 `WorkspacePresentationMode` 表示 Workspace 内部呈现。

### 3.2 顶部 UI

Workspace 顶栏常量 `TOP_BAR_HEIGHT` 为 54px，渲染入口为 `render_workspace_top_bar`：[`crates/app/src/app/chrome/top_bar.rs`](../../crates/app/src/app/chrome/top_bar.rs#L12)、[`crates/app/src/app/chrome/top_bar.rs`](../../crates/app/src/app/chrome/top_bar.rs#L44)。当前结构是全宽顶栏，中间项目标题，右侧 Launchpad 与保存状态，同时将顶栏声明为窗口拖拽区域。

Workspace Chrome 在同一层组合 minimap、command dock、top bar、通知和布局菜单：[`crates/app/src/app/chrome.rs`](../../crates/app/src/app/chrome.rs#L365)。Canvas 自动布局预留的顶部 safe area 为 86px：[`crates/app/src/canvas/layout_strategy.rs`](../../crates/app/src/canvas/layout_strategy.rs#L14)。

因此首选布局是复用 54px 顶栏：

```text
┌──────────────────────────────────────────────────────────────────────┐
│  Project / Workspace       [ Canvas | Live Review ]       Save / Home│
└──────────────────────────────────────────────────────────────────────┘
```

需要注意：顶栏本身是 window drag area，Segmented Control 必须验证子元素能稳定覆盖拖拽命中；交互态至少包括 hover、pressed、keyboard focus、selected 与 disabled。选中态不能只靠颜色表达。新增第二行子栏虽然也可行，但会进一步侵占当前 86px safe area，暂不推荐。

### 3.3 Canvas Document、Runtime 与输入路由

Canvas `Document` 只包含节点与 viewport，Snapshot 同样只序列化这两类数据：[`crates/canvas/src/document.rs`](../../crates/canvas/src/document.rs#L16)、[`crates/canvas/src/document.rs`](../../crates/canvas/src/document.rs#L22)。节点类型目前包括 Rect、Term、Agent、Preview 和 Browser：[`crates/canvas/src/node.rs`](../../crates/canvas/src/node.rs#L69)。

`Runtime` 拥有 Document、Selection、FocusState、LiveNode 注册表、运行时事件和 Workspace/session 信息：[`crates/runtime/src/runtime.rs`](../../crates/runtime/src/runtime.rs#L32)。`restore_canvas_snapshot` 会替换当前 Document：[`crates/runtime/src/runtime.rs`](../../crates/runtime/src/runtime.rs#L102)。模式切换不应调用它，也不应创建第二个 Runtime。

当前输入焦点只有两种状态：`CanvasSelect` 和 `NodeFocused(NodeId)`：[`crates/runtime/src/focus.rs`](../../crates/runtime/src/focus.rs#L6)。纯函数 `route(mode, event)` 决定 Canvas 点击、节点聚焦、键盘事件与 Master Layout 路由：[`crates/runtime/src/route.rs`](../../crates/runtime/src/route.rs#L44)。现有 route 没有 Live Review 或 Context Brush 语义。

Canvas 指针交互在 app 层执行。`InteractionState` 包含平移、拖拽、文本选择、终端链接、Preview pan 和菜单等瞬态状态：[`crates/app/src/canvas/interaction.rs`](../../crates/app/src/canvas/interaction.rs#L15)。`on_mouse_move` 明确是高频路径，并用约 16ms 的通知节流保护性能；drag/pan 只在结束时标记持久化：[`crates/app/src/canvas/events.rs`](../../crates/app/src/canvas/events.rs#L26)、[`crates/app/src/canvas/events.rs`](../../crates/app/src/canvas/events.rs#L237)、[`crates/app/src/canvas/events.rs`](../../crates/app/src/canvas/events.rs#L440)。

结论：

- Workspace 模式和视觉捕获属于 `app` 层。
- Brush 的纯状态转移、归一化几何和路由决策可以沉入零 GPUI 的域层。
- 画笔移动轨迹应只存在内存中；pointer up 后才提交、截图和调度持久化。
- Live Review 屏幕布局不能回写 `Document.nodes[*].bounds`，否则切回 Canvas 会改变用户原布局，并可能触发 PTY resize。

### 3.4 Browser 与 Preview 能力

`BrowserStore` 管理 Native Wry/CEF 子视图，可同步视图、隐藏视图、释放输入并只保留当前可见节点：[`crates/app/src/app/browser/store.rs`](../../crates/app/src/app/browser/store.rs#L17)、[`crates/app/src/app/browser/store.rs`](../../crates/app/src/app/browser/store.rs#L70)、[`crates/app/src/app/browser/store.rs`](../../crates/app/src/app/browser/store.rs#L139)、[`crates/app/src/app/browser/store.rs`](../../crates/app/src/app/browser/store.rs#L160)。Canvas 每帧结束会调用 `finish_browser_view_sync` 维护可见集合：[`crates/app/src/canvas/view.rs`](../../crates/app/src/canvas/view.rs#L451)、[`crates/app/src/app/browser.rs`](../../crates/app/src/app/browser.rs#L192)。

Native Browser 是 AppKit 子视图，并非普通 GPUI 图片。如果模式切换后不再运行 Canvas 的可见集合收尾，旧 WebView 可能继续盖在 Live Review 上。因此切换时必须：

1. 释放 Browser first responder；
2. 隐藏或 `retain_visible(empty)`；
3. 切回 Canvas 后由现有同步流程恢复。

当前 BrowserStore 没有通用截图 API，不能假设 Web 页面像素已暴露给 Context Brush。首期可把浏览器所在的 macOS 窗口当通用屏幕捕获源；更精确的 Browser backend snapshot/DOM anchor 属于后续工作。

Preview 当前支持 Markdown、Text、Image，并维护独立 Store：[`crates/app/src/preview/mod.rs`](../../crates/app/src/preview/mod.rs#L16)、[`crates/app/src/preview/mod.rs`](../../crates/app/src/preview/mod.rs#L85)。它适合在截图完成后显示确认缩略图或历史产物，但不是实时画面源。

### 3.5 已有 Ego 屏幕捕获链路

macOS app crate 已依赖 `screencapturekit`：[`crates/app/Cargo.toml`](../../crates/app/Cargo.toml#L112)。Ego Live Preview 已实现一条可以复用的低拷贝链路：

- 捕获配置、24 FPS、队列深度 2、最长边限制和 420f：[`crates/app/src/app/ego_preview/live.rs`](../../crates/app/src/app/ego_preview/live.rs#L39)
- 丢弃积压帧与乱序帧的 `LatestSequenceMailbox`：[`crates/app/src/app/ego_preview/live.rs`](../../crates/app/src/app/ego_preview/live.rs#L74)
- 420f 双平面与 IOSurface 校验：[`crates/app/src/app/ego_preview/live.rs`](../../crates/app/src/app/ego_preview/live.rs#L116)
- `SCShareableContent` 枚举并选择目标窗口：[`crates/app/src/app/ego_preview/live.rs`](../../crates/app/src/app/ego_preview/live.rs#L452)
- GPUI 使用 `surface(preview)` 渲染，而不是每帧 CPU Image：[`crates/app/src/cards/browser/card.rs`](../../crates/app/src/cards/browser/card.rs#L97)

还存在一次性窗口截图 fallback：先枚举 Ego 窗口，再调用系统 `screencapture` 生成临时 PNG 并解码：[`crates/app/src/app/ego_preview/snapshot.rs`](../../crates/app/src/app/ego_preview/snapshot.rs#L89)。刷新和 snapshot 工作已放到后台：[`crates/app/src/app/ego_preview.rs`](../../crates/app/src/app/ego_preview.rs#L145)。

边界也很明确：

- 当前目标窗口选择写死在 Ego 的 bundle/title 识别逻辑中，不是通用窗口捕获服务。
- `EgoLivePreviewStore` 与内部帧类型是 `pub(super)`，尚未形成跨来源接口。
- 当前实现只读，不把 GPUI 指针或键盘转发给 Ego；既有产品 Spec 也把它定义为只读边界：[`docs/product-specs/2026-07-29-SPEC-ego-lite-canvas-preview-card.md`](../product-specs/2026-07-29-SPEC-ego-lite-canvas-preview-card.md#L1)。
- 当前仓库未实现通用系统窗口选择器。不能把“依赖库可能具备的 API”当作已完成能力。

应抽取复用捕获机制，而不是复制 Ego 文件：目标识别/选择器与捕获会话解耦；420f 校验、最新帧邮箱、重试和暂停策略可共用。ScreenCaptureKit、CoreVideo 和 GPUI `surface()` 均继续留在 `app` 层。

性能脚本已经检查 Ego 的 420f、latest-frame mailbox 和 `surface()` 路径：[`scripts/check-video-wallpaper-performance.py`](../../scripts/check-video-wallpaper-performance.py#L274)。通用 Live Surface 必须扩展同一护栏。

### 3.6 Agent 上下文与任务分发现状

当前 `CurrentCanvasContext` 只描述 Canvas 卡片的文本元数据，包括 node id、kind、title、selected/focused：[`crates/agent/src/current_context.rs`](../../crates/agent/src/current_context.rs#L33)。

`NormaSession::turn_streaming` 接受字符串输入，内部 `TurnRequest` 最终调用 `stream_prompt(&input)`：[`crates/agent/src/master.rs`](../../crates/agent/src/master.rs#L167)、[`crates/agent/src/master.rs`](../../crates/agent/src/master.rs#L322)、[`crates/agent/src/master.rs`](../../crates/agent/src/master.rs#L355)。协调层的 `AgentAssignment` 也只包含 target 和 task 字符串：[`crates/agent/src/coordination.rs`](../../crates/agent/src/coordination.rs#L44)。

实际 Coding Agent 分发通过 `NodeInput::Paste(task.to_string())` 加 Enter 把任务输入 PTY：[`crates/app/src/app/agent_runtime/command_support.rs`](../../crates/app/src/app/agent_runtime/command_support.rs#L21)。审计存储同样只持久化 `original_instruction` 与 `task_text` 字符串：[`crates/storage/src/agent_dispatch_store.rs`](../../crates/storage/src/agent_dispatch_store.rs#L31)。

仓库有手动剪贴板图片处理：Claude/Codex 可走原生粘贴，其他 CLI 将图片保存到系统临时目录再粘贴路径；临时图片最多保留 24 小时：[`crates/app/src/app/clipboard_image.rs`](../../crates/app/src/app/clipboard_image.rs#L11)、[`crates/app/src/app/clipboard_image.rs`](../../crates/app/src/app/clipboard_image.rs#L78)。这不能直接等同于“可编排的视觉上下文”。

因此首期必须诚实定义为**文件引用型视觉上下文**，而不是原生多模态：

```text
[Visual Context]
id: ctx-20260808-001
source: com.apple.iphonesimulator / "iPhone 17 Pro"
region: x=0.31, y=0.18, width=0.42, height=0.27
captured_at: 2026-08-08T10:30:21+08:00
image: /workspace/.multisoul/context-assets/ctx-20260808-001.png
instruction: 把这块登录按钮调整成更明确的主行动按钮
```

同一个文本块可进入 Norma prompt 和每个 Agent task；Agent 可读取图片文件。未来若要让模型直接理解图片，需要在 Rig / Provider 边界增加 typed image attachment、能力协商和供应商适配，不能只改 UI。

### 3.7 持久化现状

CanvasStore 当前持久化一份 `DocumentSnapshot`：[`crates/storage/src/canvas_store.rs`](../../crates/storage/src/canvas_store.rs#L1)。`LayoutMode` 已被用于 `Tiled / MasterDetail`，`LayoutStateSnapshot` 保存 Master Layout 状态与原始 bounds：[`crates/storage/src/layout_store.rs`](../../crates/storage/src/layout_store.rs#L10)、[`crates/storage/src/layout_store.rs`](../../crates/storage/src/layout_store.rs#L22)。不能复用或重载这个 `mode` 表示 Canvas / Live Review。

项目库位于 `<workspace>/.multisoul/project.sqlite`：[`crates/storage/src/project_store.rs`](../../crates/storage/src/project_store.rs#L18)。`.multisoul` 已被 Git 忽略：[`.gitignore`](../../.gitignore#L13)。现有持久化调度器使用 360ms debounce，并在 dirty 时保存 Canvas 与布局：[`crates/app/src/workspace/persistence.rs`](../../crates/app/src/workspace/persistence.rs#L8)、[`crates/app/src/workspace/persistence.rs`](../../crates/app/src/workspace/persistence.rs#L45)。

建议：

- P0 让当前 Workspace mode 保持内存态，降低恢复与迁移风险。
- 需要恢复时，新增独立 `workspace_presentation_state`，不要扩展 `layout_state.mode`。
- Context 图片落入 `<workspace>/.multisoul/context-assets/`，数据库只保存相对路径、来源提示、归一化区域、指令和时间戳；不要把图片 base64/BLOB 塞入 task、日志或 SQLite。
- 外部 window id 只在当前系统会话有效。持久化时只保留 bundle id、标题提示和最近区域，重启后必须重新匹配并让用户确认。
- 只在 brush commit、remove/unpin 或显式模式偏好变化时标记 dirty；绝不能在 mouse move 热路径写库或扫描目录。

## 4. 建议架构

### 4.1 所有权与依赖边界

```text
crates/app
  WorkspacePresentationMode / LiveReviewState
  Live Review GPUI view + pointer events + overlays
  macOS window picker / ScreenCaptureKit session / CoreVideo surface
  background snapshot crop + asset write
                  │
                  ├──────────────┐
                  ▼              ▼
crates/runtime                   crates/agent
  pure input route                VisualContextRef
  brush lifecycle decisions       text projection / future attachment seam
                  │              │
                  ▼              ▼
crates/canvas                    crates/storage
  normalized geometry             presentation/context metadata
  source-independent region       project DB migration/store
```

约束：

- `canvas` / `runtime` 不得依赖 GPUI、ScreenCaptureKit、Storage 或 App。
- ScreenCaptureKit window handle、CVPixelBuffer、IOSurface、`surface()` 留在 `app`。
- `storage` 只依赖可序列化的域类型，不认识 GPUI Entity 或 native window object。
- `agent` 只接收可移植的视觉上下文描述，不依赖 app 的捕获对象。
- 同一个 `Runtime`、Agent coordinator、BrowserStore 和 PTY session 由两种模式共享。

### 4.2 建议模块

模块名是建议，不代表当前已存在：

```text
crates/app/src/app/workspace_mode.rs
  WorkspacePresentationMode
  enter_live_review / leave_live_review 的清理与恢复

crates/app/src/app/live_review/
  state.rs        来源、brush、chat/context chips、crew projection
  view.rs         中央 live surface 与三栏/浮层布局
  events.rs       pointer/keyboard → pure route → app effect
  layout.rs       screen-space 布局，不修改 Canvas Document bounds

crates/app/src/app/live_surface/
  source.rs       Canvas node / ExternalWindow 的 app 级 descriptor
  store.rs        active source、frame、错误、暂停/重连
  macos_capture/
    picker.rs     基于 SCShareableContent 的 App 内窗口选择器
    session.rs    通用低拷贝捕获会话
    mailbox.rs    从 Ego 抽取的 latest-frame 机制
    snapshot.rs   单次 BGRA/PNG 捕获与后台裁剪

crates/runtime/src/review_route.rs
  ReviewInputMode / ReviewEvent / ReviewRoute

crates/canvas/src/context_region.rs
  ContextRegionId / NormalizedRect / selection state

crates/agent/src/visual_context.rs
  VisualContextRef / 文本投影；未来 typed attachment seam

crates/storage/src/context_region_store.rs
  context metadata、相对 asset path、retention
```

### 4.3 Workspace 模式状态

建议新增：

```rust
enum WorkspacePresentationMode {
    Canvas,
    LiveReview,
}
```

它应放在 `MultisoulApp` 附近，而不是 `canvas::Document`。根渲染仍进入 `AppScreen::Canvas` 分支；缓存 Workspace Surface 根据 mode 选择 `render_canvas` 或 `render_live_review`，Workspace Chrome 继续共用。

模式切换需要一个原子化 transition：

```text
Canvas → Live Review
1. 取消 drag / pan / text selection / pending link / menus
2. 记录当前 runtime focus 作为可选恢复提示
3. release native Browser input，隐藏 Native WebViews
4. 将 keyboard/IME 路由切到 Review / Chat / Brush
5. 激活 Live Surface；不改 Document / viewport / node bounds

Live Review → Canvas
1. 取消未提交 brush 和捕获中的 transient UI
2. 停止或暂停只属于 Live Review 的外部捕获源
3. 恢复 Canvas render 与 Browser visible sync
4. 若原 focused node 仍存在，再恢复焦点；否则回到 CanvasSelect
5. 不重启 PTY / Agent / Runtime
```

关键风险是：若仍保持 `AppScreen::Canvas`，既有 keyboard/IME 门控也仍为真。实现必须增加 Workspace mode 的输入门控，或将 Review 状态纳入纯 runtime route；否则隐藏的 `NodeFocused` Terminal 仍可能收到用户在 Live Review 键入的内容。

### 4.4 Live Review 只做另一种投影

Live Review 的 Agent Crew 卡片首期应读取现有 Runtime/协调器状态，显示名称、任务摘要、状态、最近结果和是否需要用户介入。它们是 read model，不是第二套 Agent 节点，也不应修改原 Canvas 卡片尺寸。

原因是 Terminal/Agent 卡片的显示尺寸可能驱动 PTY resize。若为 Live Review 直接复用同一完整 Card Body 并换一个较小矩形，有机会影响 Canvas 中原会话的终端尺寸。首期使用紧凑状态投影；后续若需要同一节点多视图，应先引入不改变 live node geometry 的 presentation adapter。

### 4.5 Live Surface 来源接口

建议 source identity 与 frame delivery 分离：

```rust
enum LiveSurfaceSourceRef {
    CanvasNode(NodeId),
    ExternalWindow {
        bundle_id_hint: Option<String>,
        title_hint: String,
        volatile_window_id: Option<u32>,
    },
}
```

`volatile_window_id` 只能用于当前会话。外部窗口 picker 首期可以直接基于 `SCShareableContent` 枚举出的应用和窗口构建 Norma 自己的选择面板。仓库中未验证 native system picker binding，因此不把它列为 P0 前提。

来源优先级：

1. **Ego**：已有完整实时链路，最适合先抽象验证。
2. **任意外部窗口**：抽取通用捕获后接入。
3. **iPhone Simulator**：作为外部窗口接入，不先做专用协议。
4. **Canvas Browser Node**：先用外部窗口捕获；后续补 backend snapshot 或 DOM 语义锚点。
5. **Preview/Image**：可直接作为静态来源，无需启动 ScreenCaptureKit。

捕获会话沿用现有原则：最长边约束、24 FPS、queue depth 2、latest-frame wins、420f + IOSurface + `surface()`。窗口消失、最小化、权限拒绝和 App inactive 时保留最后一帧并给出明确状态，不在 UI 线程重试或 sleep。

### 4.6 Context Brush 数据模型

建议纯类型：

```rust
struct ContextRegionId(String);

struct NormalizedRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

enum ContextCaptureMode {
    Snapshot,
    LiveRegion,
}

struct VisualContextRef {
    id: ContextRegionId,
    source: VisualSourceDescriptor,
    region: NormalizedRect,
    capture_mode: ContextCaptureMode,
    asset_relative_path: Option<PathBuf>,
    captured_at_unix_ms: Option<i64>,
    user_instruction: String,
}
```

坐标必须以**捕获内容区域**为基准归一化到 0..1，而不是保存屏幕像素。这样来源窗口缩放后仍能在近似位置重新裁剪。实现仍需处理 letterbox：先从 Live Surface 屏幕坐标反算到实际视频内容矩形，再归一化并 clamp。

首期建议 UI 可以表现为自由画笔，但产物先取画笔包围盒的矩形 crop；真正 polygon alpha mask 延后。否则会同时引入路径简化、遮罩栅格化、边缘抗锯齿和模型兼容问题。

### 4.7 Brush → Agent 的数据流

```text
Pointer Down
  → app 内存中开始 stroke
Pointer Move
  → 更新 overlay，最多约 16ms notify；无 IO、无 SQLite
Pointer Up
  → pure route 产出 CommitRegion
  → 计算 content rect 与 NormalizedRect
  → 后台做一次全窗口 snapshot / crop / PNG write
  → UI 显示 Context Chip + 缩略图，请用户确认
  → 用户发送指令
  → VisualContextRef 渲染成文本块
  → Norma prompt + 每个 AgentAssignment.task 引用同一 Context ID/path
  → Agent 状态与结果回到 Live Review Crew
```

为什么不直接裁 420f live frame：当前实时链路是为 `surface()` 上屏设计的，仓库没有可复用的 420f → PNG crop API。首期在 brush commit 时后台触发一次 BGRA/PNG snapshot 最稳妥，但实时画面与 snapshot 之间可能有几十到几百毫秒时间差。UI 应在提交瞬间短暂冻结或显示“已捕获”缩略图，让用户确认实际发送的像素。

`LiveRegion` 的首期语义可以是“每次发起新任务时，按同一 NormalizedRect 重新截图”，而不是让像素区域语义追踪某个按钮。要跟随 Web DOM 或 Accessibility Element，必须新增来源专用 anchor adapter。

## 5. 风险与缓解

### 5.1 输入泄漏到隐藏 Terminal

风险：Workspace 仍是 `AppScreen::Canvas`，当前 Node focus 仍可能接收键盘/IME。

缓解：模式切换时清理/隔离 node focus；keyboard、IME、Tab traversal 和 paste 路由都显式检查 `WorkspacePresentationMode`；为“Live Review 输入不进入 PTY”添加 E2E 正负断言。

### 5.2 Native Browser 残留覆盖

风险：Wry/CEF 是 native child view，不随 GPUI 分支自然消失。

缓解：切换事务中调用 release input 和 hide/empty retain；切回后让现有 Canvas sync 恢复。真实 App 验证不可只依赖 headless GPUI。

### 5.3 捕获权限和窗口生命周期

风险：用户拒绝 Screen Recording；窗口消失、最小化、换标题或重建 window id。

缓解：把 Permission Required、Disconnected、Paused、Retrying 作为一等状态；保留最后一帧；重连基于 bundle/title hint，但由用户确认，不能静默捕获另一个同名窗口。

### 5.4 自捕获与递归画面

风险：用户选择 Norma OS 自己的窗口，会形成递归镜像；选 Browser 时可能把顶栏和 Context Brush 一并捕获。

缓解：picker 默认过滤当前主窗口，提供高级显示；对 Browser 精确画面后续建立 backend snapshot source。

### 5.5 性能回退

风险：为了裁剪或缩略图，把每个 420f frame 转为 CPU RGBA/RenderImage。

缓解：实时路径只走 IOSurface `surface()`；PNG 编码仅在 commit/recapture 后台发生；沿用 latest-frame mailbox；扩展性能脚本覆盖通用 Live Surface。

### 5.6 截图时序偏差

风险：用户圈住的是画面 A，后台一次性截图拿到画面 B。

缓解：pointer up 时冻结显示、立即 snapshot，完成后用最终 crop 替换 overlay 并要求确认；若误差不可接受，再研究从最新 CVPixelBuffer 做单帧 BGRA 转换，而不是每帧转换。

### 5.7 隐私、凭据与留存

风险：外部窗口可能包含密钥、客户数据或通知；图片路径可能进入任务审计。

缓解：只有明确用户手势才捕获；持续显示来源与录制状态；Context Chip 可移除；支持项目级 retention；日志不写图片内容/base64；删除上下文时同步删除 sidecar asset；跨 Agent 扇出前展示将共享给哪些 Agent。

### 5.8 Agent 实际看不懂图片

风险：文件路径能传到 Agent，但某个 CLI/模型没有视觉能力，或者没有主动读取文件。

缓解：任务模板明确写“先读取此图片文件”；协调器记录 Agent capability；没有视觉能力时由 Norma 先生成文字描述，或提示用户更换 Agent。真正的 typed image attachment 列为后续架构升级。

### 5.9 外部窗口不可编辑

风险：捕获只提供观察，用户可能误解为 Agent 能直接修改任意 App。

缓解：产品文案区分“正在查看的结果”与“Agent 正在修改的代码/文件”。外部 App 需要自身 reload/hot reload 才显示新结果；当前没有对外部 App 的输入转发和自动控制实现。

## 6. 分阶段 MVP

### Slice 0：模式骨架与输入安全

- 顶栏新增 `Canvas / Live Review` segmented control。
- 新增 app 级 `WorkspacePresentationMode`。
- 同一 Runtime 下显示空的 Live Review shell。
- 切换不修改 Document、viewport、node bounds，不重启 PTY/Agent。
- 完成 transient Canvas interaction、Browser native view、focus/keyboard/IME 的切换清理。
- P0 mode 只保存在内存。

验收门槛：来回切换 20 次，Canvas 节点位置、viewport、Terminal session 和 Agent 执行状态保持不变；Live Review 键入内容不进入隐藏 PTY。

### Slice 1：Ego Live Surface + Crew Read Model

- 从 Ego 实现抽取通用低拷贝 capture session，但先仍只接 Ego source。
- 中央区域展示实时 Ego surface、来源标签、捕获状态。
- 左/右侧以紧凑投影展示现有 Agent 状态和任务，不创建重复 Agent node。
- 保持 420f、IOSurface、latest-frame、queue depth 2、后台重试。

验收门槛：真实 App 运行中，Ego 画面更新；Agent/PTTY 不受预览尺寸影响；性能护栏通过。

### Slice 2：通用 macOS Window Source

- 基于 `SCShareableContent` 增加 App 内窗口 picker。
- 支持普通桌面 App 与 iPhone Simulator 窗口。
- 支持权限拒绝、窗口消失、最小化、重连与换源。
- 默认过滤 Norma 当前主窗口，避免递归捕获。

验收门槛：真实 Screen Recording permission allowed/denied 两条 L3 路径都有证据；目标窗口关闭后 UI 明确显示断开且不会卡 UI 线程。

### Slice 3：Rectangle Snapshot Brush

- 增加 brush/crop overlay、Esc 取消和 clear action。
- pointer up 后后台 snapshot + crop，写入 `.multisoul/context-assets/`。
- Context Chip 显示来源、缩略图、时间、区域和删除按钮。
- 下一轮 Norma 指令包含 `VisualContextRef` 文本投影。

验收门槛：每次 commit 只创建一个 asset；选择移动时不写文件/SQLite；用户确认的 crop 与最终 Agent 引用文件一致。

### Slice 4：多 Agent 共享视觉上下文

- 每次 fan-out 使用同一个 Context ID、asset path 和原始用户指令。
- task lineage 与结果 UI 显示 Context ID。
- 无视觉能力的 Agent 有明确降级或阻止策略。
- 增加 context remove/retention 和审计语义。

验收门槛：至少两个 Agent 的持久化 task_text 都包含相同 Context ID/path；未选择的 Agent 不收到该上下文；删除后图片与元数据按策略一致清理。

### 后续阶段

- 自由画笔 polygon mask。
- `LiveRegion` 每轮自动重捕获。
- Browser DOM、Accessibility Element 或 Figma Layer 语义锚点。
- Provider 原生 multimodal attachment。
- Workspace mode、来源与 pinned context 的恢复。
- 可选的外部 App 操作/自动化能力；需单独做权限与安全设计。

## 7. 自动化与验收证据

### 7.1 纯单元测试

- `WorkspacePresentationMode` 切换前后同一 Runtime/Document snapshot 不变。
- review route 的 brush begin/update/commit/cancel。
- screen rect → content rect → normalized rect 的 letterbox、clamp 和 source resize。
- 失效 window id 不被当作持久身份恢复。
- Context ID 和相对 asset path 的序列化/反序列化。
- 多 Agent task projection 使用同一 context，未选 Agent 无 context。

所有新增测试必须遵守仓库的完整场景注释、数值推导、执行步骤、正负断言与断言说明规范。

### 7.2 Headless E2E

- 顶部 Tab 的 hover、pressed、keyboard focus、selected、disabled 状态。
- Tab 切换后 Canvas Document、viewport 与 Agent task state 保持。
- Live Review 中 key/IME/paste 不发送给隐藏 Terminal。
- Context Brush 只在 commit 后产生一份 asset/reference。
- 多 Agent fan-out 收到相同 context ID，非目标 Agent 没有收到。

### 7.3 真实 App L3 / Computer Use

- 顶栏子控件与 window drag area 的真实命中。
- Native Wry/CEF 切换后不覆盖 Live Review，first responder 已释放。
- Screen Recording 权限允许与拒绝。
- Ego、普通 App、Simulator 三种窗口实时画面。
- 窗口关闭、最小化、重建后状态与重连。
- 实际框选 crop、确认图与发送给 Agent 的文件一致。
- 缩放、Retina、窗口 resize 和 letterbox 下框选位置正确。

页面、布局和视觉状态的最终验收必须使用 Computer Use 操作真实 App；静态截图和 headless GPUI 只能补充。

### 7.4 性能证据

- 扩展 [`scripts/check-video-wallpaper-performance.py`](../../scripts/check-video-wallpaper-performance.py#L274)，覆盖通用 Live Surface。
- `on_mouse_move` 不含文件 IO、SQLite、窗口枚举或同步截图。
- 实时捕获不走 `RenderImage` / `img()` CPU frame 路径。
- mailbox 有界，旧帧丢弃；Source 关闭后会话资源释放。
- 模式切换不 spawn 新 PTY、不重新启动 Agent，不写 Canvas snapshot。

## 8. 需要产品确认的决策

这些问题不阻挡 Slice 0，但会影响 Slice 3/4：

1. **默认捕获语义**：建议默认 Snapshot；LiveRegion 作为显式可选项。
2. **发送时机**：建议框选后先出现可删除 Context Chip，只有用户发送对话时才共享给 Agent。
3. **共享范围**：建议默认只给 Norma；用户选中/Norma 明确委派的 Agent 才收到，不默认广播所有 Agent。
4. **图片留存**：建议跟随项目保存，提供“会话结束删除 / 7 天 / 永久”策略；首期可先采用会话级显式清理。
5. **模式恢复**：建议 P0 每次进入 Workspace 默认 Canvas；验证稳定后再记住上次 mode/source。
6. **Crew 布局**：首期优先显示 Agent 状态、任务和最近结果，不在 Live Review 中嵌入完整可输入 Terminal。
7. **画笔形态**：首期交互可用自由画笔视觉，但发送内容以包围矩形 crop 为准，并在 UI 中明确显示最终范围。

## 9. 建议的实现决策

可以据此进入设计与原型的最小决策集：

- 采用 app 级 `WorkspacePresentationMode`，不新增 `AppScreen::LiveReview`。
- 复用一个 Runtime；Live Review 是投影，不是第二份 Canvas Document。
- 先复用/抽象 Ego 的 420f 低拷贝链路，再做通用窗口 picker。
- Context Brush 首版采用矩形 Snapshot；后台截图、裁剪和落盘。
- Agent 首版通过 `VisualContextRef` 文本 + workspace-local PNG 路径接收上下文。
- Context 图片落到 `.multisoul/context-assets/`，不复用 24 小时系统剪贴板临时目录。
- 不在 pointer move、frame delivery 或无 dirty flush 上做 SQLite/文件 IO。
- 先完成输入隔离、Native Browser 清理和真实 App E2E，再扩展持久化与原生多模态。

基于当前代码，这条方案不需要推翻 Canvas、Runtime 或 Agent Coordinator；真正需要新建的是 Workspace 呈现状态、通用 Live Surface 捕获适配层、Context Brush 域模型，以及从视觉文件引用到 Agent task 的桥接层。
