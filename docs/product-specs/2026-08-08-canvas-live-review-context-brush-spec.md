# Canvas Live Review 与 Context Brush SPEC

> 日期：2026-08-08  
> 状态：Draft / 可行性已确认，尚未进入实现  
> 范围：画布顶部模式切换、实时执行结果预览、区域上下文、多人指挥多 Agent  
> 原型参考：[01-live-crew-context-brush.png](../design-docs/assets/norma-context-brush-prototype/01-live-crew-context-brush.png)  
> 技术调研：[Canvas Mode + Context Brush 可行性研究](../research/2026-08-08-canvas-mode-context-brush-feasibility.md)

## 1. 结论

该方向在当前架构上可行，且不需要创建第二套 Canvas、Runtime 或 Agent 系统。

推荐把“当前画布”和“新画布”定义为同一个 Workspace 的两种呈现模式：

- **Canvas**：现有无限画布，负责节点编排、Terminal、Browser 与 Agent 卡片的自由布局。
- **Live Review**：以正在运行的页面、手机模拟器或桌面窗口为中心，用户可直接圈选画面区域，把它作为可追踪的视觉上下文交给 Norma 和指定 Agent。

两种模式共享同一个 `Document`、`Runtime`、PTY 会话、Agent Coordinator 和持久化项目。顶部 Tab 只改变 Workspace 的投影和交互策略，不复制业务状态。

首版应采用“实时画面 + 提交时截图”的模型：画面持续低拷贝预览，但只有用户完成圈选并发出指令时才生成静态上下文资产。不要在首版持续向模型传视频帧，也不要承诺跨页面变化的语义目标追踪。

## 2. 第一性原理审查

用户真正需要完成的闭环不是“拥有另一张画布”，而是：

```text
看到 Agent 的真实执行结果
        ↓
准确指出画面中的问题
        ↓
用自然语言说明修改意图
        ↓
让一个或多个 Agent 获得相同、无歧义的上下文
        ↓
在同一画面中验证新的结果
```

因此，新模式必须优先解决四件事：

1. 结果必须持续可见，而不是只能阅读 Agent 日志。
2. 用户必须能直接操作画面来表达“这里”，而不是人工描述坐标或控件名称。
3. 视觉上下文必须能被确定性地附加到任务，不能依赖模型记得复制图片路径。
4. 多 Agent 的分工、执行状态和产出必须在结果画面周围可见，但不能遮挡主要内容。

仅增加一个 Tab，而没有视觉上下文闭环，只是导航变化；仅提供截屏，而不与 Agent 任务建立可追踪关系，也不能满足目标。

## 3. 产品定义

### 3.1 工作名称

顶部模式名称暂定：

- `Canvas`
- `Live Review`

“Preview”容易让人理解为只读页面预览，而新模式还包含圈选、对话和多 Agent 指挥，因此更推荐 `Live Review`。最终命名可在实现前进行一次文案验证。

### 3.2 核心用户故事

作为使用 Norma 进行 vibe coding 的用户，我希望：

- 在画布顶部一键进入 Live Review，而不丢失当前节点、终端与 Agent 状态。
- 在中心区域实时看到 Web 页面、iPhone Simulator、桌面程序或已有 Preview 节点的运行结果。
- 用画笔框住某个区域，然后直接说“把这里的按钮改得更轻一些”。
- 在发送前看到 Norma 将共享哪张图、哪个区域、发给哪些 Agent。
- 让 Norma 把同一个视觉上下文分派给一个或多个 Agent，并看到每个 Agent 的状态和结果。
- 随时切回 Canvas，继续操作原来的节点和 Terminal。

### 3.3 非目标

首版不包含：

- 远程点击、键盘控制或自动操作被捕获的外部 App。
- 持续上传整个屏幕的视频流给模型。
- 自动理解并持续追踪“这个按钮”在页面重排后的新位置。
- 任意形状的像素级蒙版编辑。
- 在 Live Review 中复制一套完整 Canvas 节点和 PTY 实例。
- 默认把用户圈选内容广播给所有 Agent。

## 4. 体验设计

### 4.1 顶部模式切换

在现有 54px 顶栏中加入 macOS 风格的 segmented Tab，优先复用 `gpui-component 0.5.1` 的 `TabBar::segmented()`。

推荐顶栏结构：

```text
┌──────────────────────────────────────────────────────────────────────────┐
│  Project / Workspace       [ Canvas | Live Review ]      Save   Launchpad │
└──────────────────────────────────────────────────────────────────────────┘
```

交互要求：

- 当前模式必须有明确 selected 状态。
- 两个 Tab 都必须覆盖 hover、pressed、keyboard focus 和 disabled 状态。
- 点击 Tab 的区域不能继续作为窗口拖拽区域。
- Tab 之外的空白顶栏仍可拖动窗口。
- 支持键盘切换，具体快捷键必须在 Settings → Keyboard Shortcuts 中可配置。
- 模式切换应即时完成，不播放阻塞性的转场动画。
- Reduce Motion 开启时只使用颜色、描边或短淡入淡出表达状态变化。

这体现 Apple 设计原则中的内容优先、直接操作、即时反馈和用户可控。

### 4.2 Live Review 布局

```text
┌──────────────────────────────────────────────────────────────────────────┐
│  Project / Workspace       [ Canvas | Live Review ]      Save   Launchpad │
├───────────────┬──────────────────────────────────────────┬───────────────┤
│ Crew          │                                          │ Context       │
│               │          Live Surface                    │               │
│ Norma         │   Web / Simulator / Desktop Window       │ Selected #1   │
│ Designer  ●   │                                          │ Target: 2     │
│ Frontend  ◐   │        ╭─ Context Brush Region ─╮        │ Snapshot      │
│ QA        ○   │        ╰────────────────────────╯        │ Remove        │
│               │                                          │               │
├───────────────┴──────────────────────────────────────────┴───────────────┤
│  Ask Norma…                     Context attached · Send to 2 agents   Send │
└──────────────────────────────────────────────────────────────────────────┘
```

布局原则：

- 中心 Live Surface 是第一视觉层级，尽量占用最大面积。
- Crew 是 Agent 状态的紧凑读模型，不渲染第二套完整 Terminal 卡片。
- Norma 是唯一的主对话入口；其他 Agent 展示角色、任务、状态和最近产出。
- Context 面板显示已选择的来源、快照、区域和共享目标。
- 窄窗口下优先折叠 Context 面板，其次把 Crew 收成浮动列表，不压缩 Live Surface 到不可用尺寸。

### 4.3 来源选择

首版来源优先级：

1. 当前 Workspace 中已有的 Browser / Preview Live Node。
2. 已有 Ego 预览来源。
3. macOS 外部窗口，包括 iPhone Simulator。
4. 静态图片或最近一次快照。

外部窗口捕获是只读投影。用户需要与外部 App 交互时，应切换到该 App；Norma 不在首版注入点击或键盘事件。

### 4.4 Context Brush

默认手势采用矩形框选。可以保留画笔般的描边反馈，但首版实际提交的是描边包围盒对应的矩形截图。

操作流程：

1. 用户激活 Context Brush。
2. 在 Live Surface 上按下并拖动。
3. 拖动过程中只更新内存中的视觉选区，不做磁盘 IO 或模型调用。
4. 松开后把屏幕坐标映射为 Live Surface 内容区域内的归一化坐标。
5. 后台获取最新帧并裁剪，写入 Context Asset。
6. UI 显示 Context Chip，包含缩略图、来源、捕获时间和目标 Agent。
7. 用户输入指令并发送。
8. Norma 和指定 Agent 获得同一个 `VisualContextRef`。

如果画面存在 letterbox，坐标必须相对真实内容矩形计算，不能包含容器留白。

### 4.5 两种捕获策略

首版支持：

- `Snapshot`：圈选后固定为当时画面，默认策略。
- `Live Region`：保存来源和归一化区域，每次发送新指令前重新截取该区域。

`Live Region` 不代表连续视频，也不代表语义跟踪。如果窗口尺寸、来源或内容布局变化导致区域可能失效，UI 必须提示用户重新确认。

## 5. 可行性矩阵

| 能力 | 可行性 | 当前基础 | 主要约束 |
|---|---|---|---|
| 顶部模式 Tab | 高 | 已有 54px 顶栏，组件库有 segmented Tab | 需拆分窗口拖拽命中区域 |
| 保留现有 Canvas 状态 | 高 | 单一 `Document`、`Runtime` 和缓存 Surface | 不能新增第二套 Runtime |
| Workspace 内 Browser / Preview | 高 | 已有节点投影与 Browser/Preview Store | 原生 WebView 必须显式隐藏或重定位 |
| Ego 实时预览 | 高 | 已有 ScreenCaptureKit + IOSurface + 420f | 当前代码与 Ego 来源耦合较紧 |
| 通用 macOS 窗口捕获 | 中高 | 已能枚举窗口和创建捕获流 | 权限、系统版本和来源选择器需产品化 |
| iPhone Simulator | 中高 | 可作为普通 macOS 窗口捕获 | 不包含 Simulator 输入控制 |
| 矩形区域截图 | 高 | 已有最新帧和 snapshot 基础 | 需正确处理缩放、Retina 与 letterbox |
| 自由画笔蒙版 | 中 | 可基于矩形模型扩展 | 需要像素蒙版与更多编辑状态 |
| 多 Agent 共享同一视觉上下文 | 中高 | 已有 Agent Coordinator 和任务分派 | 当前 Agent 输入主路径仍是文本 |
| Provider 原生多模态图片输入 | 中 | 已有手动剪贴板图片路径 | Provider 能力与 Rig 接口需另行演进 |
| 跨布局语义目标追踪 | 低 | 当前无视觉目标跟踪模型 | 不进入首版 |
| 直接操作外部 App | 低且高风险 | 当前只有捕获能力 | 涉及辅助功能权限、输入注入和安全边界 |

## 6. 架构决策

### 6.1 不新增 `AppScreen`

当前 `AppScreen` 只有 `Launchpad` 和 `Canvas`，大量键盘、IME、语音、Agent 和帧循环逻辑都以 `AppScreen::Canvas` 为前提。

如果新增 `AppScreen::LiveReview`，会把 Live Review 错误地变成另一套应用页面，并导致现有输入与运行循环被意外关闭。

推荐新增 App 层状态：

```rust
enum WorkspacePresentationMode {
    Canvas,
    LiveReview,
}
```

`AppScreen` 仍然保持 `Launchpad | Canvas`。`WorkspacePresentationMode` 只决定 Canvas Screen 内部采用哪一种投影与交互策略。

### 6.2 共享 Workspace 核心状态

以下状态在两种模式间共享：

- `canvas::Document`
- `runtime::Runtime`
- PTY session 和节点生命周期
- Agent Runtime / Coordinator
- Browser、Preview、Ego Source Store
- Workspace 路径与项目数据库

Live Review 只能读取节点和 Agent 状态形成新的 read model，不能为了布局 Crew 而修改节点 `bounds`，也不能触发隐藏 Terminal 的 PTY resize。

### 6.3 新增零 GPUI 的 `visual_context` Module

视觉上下文同时被 `app`、`agent` 和 `storage` 使用。若把坐标、来源、资产和附加规则分别散落在三个 crate 中，同一语义会被重复表达。

建议新增深 Module：

```text
crates/visual_context/
  src/
    geometry.rs       # NormalizedRect、content rect 映射、裁剪边界
    source.rs         # VisualSourceDescriptor、稳定来源标识
    manifest.rs       # VisualContextManifest、hash、时间和策略
    lifecycle.rs      # Draft / Ready / Attached / Dispatched / Removed
    lib.rs            # 小而稳定的公共 Interface
```

约束：

- 零 GPUI。
- 零 Storage。
- 零网络与系统捕获 API。
- 不保存图像二进制。
- 纯逻辑必须能独立 `cargo test -p visual_context`。

这个 Module 的 Interface 应保持小，只暴露稳定领域概念；截图、裁剪实现、窗口枚举和 UI 状态都留在上层。即使未来删除该 crate，归一化坐标、来源描述和 Context Manifest 仍会在多个调用方重新出现，因此它通过 deletion test，值得成为真实 Seam。

### 6.4 App 层适配器

建议的职责结构：

```text
crates/app/src/app/live_review/
  mode.rs             # 模式切换与临时状态清理
  view.rs             # Live Review 主投影
  crew.rs             # Agent read model 与状态呈现
  source/
    canvas_node.rs    # Browser / Preview 节点来源
    external_window.rs# ScreenCaptureKit 窗口来源
    static_image.rs   # 静态资产来源
  brush/
    interaction.rs    # 指针手势，不做 IO
    capture.rs        # 后台快照与裁剪
    overlay.rs        # 选区反馈、Context Chip
```

这些目录名表达职责，而不是使用 `utils`、`handlers` 或笼统 `views`。

来源层应形成一个真实 Seam，因为至少存在 Canvas Node、外部窗口和静态图片三个适配器。公共 Interface 只需要覆盖：

- 打开或切换来源。
- 获取当前可显示帧或原生内容句柄。
- 获取指定归一化区域的静态快照。
- 暂停、恢复和关闭来源。
- 报告权限、断开和失败状态。

实现细节如 ScreenCaptureKit 队列、IOSurface、Browser 原生子视图和文件解码必须隐藏在 Implementation 内。

### 6.5 Runtime 输入安全

模式切换不仅是渲染变化，也是输入边界变化。

从 Canvas 切入 Live Review 时必须按顺序：

1. 结束当前 drag、pan、resize、文本选择和临时菜单。
2. 让 Browser 原生子视图释放 first responder。
3. 隐藏不属于 Live Review 来源的 Wry / CEF 原生子视图。
4. 清空对隐藏 Terminal 的键盘和 IME 路由。
5. 激活 Live Review 的 source、brush 或 composer focus。

切回 Canvas 时恢复 Canvas 的可见节点投影，但不应自动恢复到可能已失效的原生文本输入状态。

`runtime::route` 继续承载纯输入意图。建议增加 Workspace 呈现模式或更通用的 interaction scope 作为路由输入，避免在 GPUI 回调中散落 `if live_review` 分支。

关键不变量：用户在 Live Review 的对话框输入文字时，隐藏的 Terminal 绝不能收到字符或 Enter。

### 6.6 Agent 上下文附加

当前主链路以文本输入为主，任务分派最终会把字符串写入 Terminal。首版不应把“图片已传给模型”与“把本地图片路径写进任务”混为一谈。

推荐引入 typed context：

```text
AgentTurn
  ├─ text
  ├─ visual_contexts: [VisualContextRef]
  └─ target_agents
```

`VisualContextRef` 至少包含：

- Context ID
- Workspace 内相对路径
- 来源类型与来源显示名
- 归一化区域
- 捕获策略和时间
- 内容 hash
- 用户可见说明

当 Norma 调用 `create_agent(task)`、`assign_agent_task` 或 `reply_agent` 时，Agent Adapter 必须确定性地把 Context Manifest 投影到目标 Agent 的任务输入中，而不是让主模型自行复制路径。

首版兼容策略：

- 支持原生图片粘贴的 Claude/Codex CLI，可以复用现有剪贴板图片输入能力。
- 其他 Agent 收到图片文件的绝对路径及简短 Context Manifest。
- Provider 原生多模态输入作为后续适配器能力，不阻塞 MVP。

默认共享规则：

- Norma 总是获得当前已确认 Context。
- 只有用户明确选择或 Norma 明确分派的 Agent 获得 Context。
- 多个 Agent 使用同一个 Context ID 和不可变 snapshot，不重复生成内容不同的副本。

### 6.7 存储模型

图像二进制不写入 SQLite，不写入 Agent 日志，也不编码为 base64 放进任务文本。

推荐布局：

```text
<workspace>/.multisoul/
  project.sqlite
  context-assets/
    <context-id>/
      snapshot.png
      manifest.json
```

数据库只保存：

- `visual_contexts`：ID、相对路径、来源、归一化区域、策略、hash、创建时间、状态。
- `agent_task_visual_contexts`：Context 与 Agent Task 的关联。

模式持久化策略：

- P0：`WorkspacePresentationMode` 仅保存在内存中，打开项目默认 Canvas。
- 后续：单独增加 `workspace_presentation_state`。
- 不得复用 `layout_state.mode`，它当前表示 `Tiled | MasterDetail`，语义不同。

Context Asset 默认跟随 Workspace 生命周期。删除 Context 或清理历史任务时应以可恢复方式移入废纸篓或延迟清理；具体保留期限在实现前确定。

## 7. macOS 捕获方案

### 7.1 复用现有低拷贝管线

当前 Ego Preview 已经验证了以下路径：

- ScreenCaptureKit 捕获。
- 420f 双平面帧。
- IOSurface-backed frame。
- GPUI `surface()` 上屏。
- 24 FPS、队列深度 2、最长边 1920px。
- latest-frame mailbox 丢弃积压帧。

Live Review 应提取其中与 Ego 无关的捕获能力，形成通用外部窗口 Source Adapter。实时画面不得退化为逐帧 CPU RGBA 转换、`RenderImage` 或 `img()` 上屏。

只有 Context Brush 提交时才允许在后台执行一次静态截图和裁剪。

### 7.2 官方能力与版本边界

Apple 将 ScreenCaptureKit 定义为高性能的显示器、应用和窗口捕获框架；窗口过滤、流式帧和 IOSurface 路径均适合本功能。[ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit)、[Capturing screen content in macOS](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos)

需要明确处理三个版本事实：

1. ScreenCaptureKit 从 macOS 12.3 开始可用，而当前打包最低版本是 macOS 12.0。
2. Apple 推荐的系统级 `SCContentSharingPicker` 需要 macOS 14+。[SCContentSharingPicker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker)、[WWDC23: What's new in ScreenCaptureKit](https://developer.apple.com/videos/play/wwdc2023/10136/)
3. 当前项目使用 `screencapturekit = 0.2.8`，已有捕获能力，但没有系统 picker API；升级到新主版本或使用原生桥接都需要单独验证，不能视为机械升级。

推荐兼容策略：

- macOS 12.0–12.2：Canvas 和 Workspace 内部 Preview 可用；外部窗口捕获显示不可用原因。
- macOS 12.3–13：使用 Norma 自己的窗口列表选择器。
- macOS 14+：优先系统 `SCContentSharingPicker`，前提是完成 crate 升级或原生桥接验证。

若产品决定只支持 macOS 14+，可简化外部来源选择与权限体验，但这是发布范围决策，不应由实现顺带改变。

### 7.3 权限与 Info.plist

外部窗口来源需要 Screen Recording 权限。正式实现前必须：

- 在 Info.plist 增加清晰的 `NSScreenCaptureUsageDescription`。
- 首次选择外部来源时解释为什么需要权限。
- 拒绝权限后提供可恢复路径和打开 System Settings 的入口。
- 权限未授予时仍允许使用 Canvas 内 Browser / Preview 来源。
- 持续显示当前正在捕获的来源名称和状态。

### 7.4 防止递归捕获

来源枚举默认排除：

- Norma 主窗口。
- Live Review 自身窗口。
- 已标记为不可共享或空标题的系统窗口。

如果用户明确选择包含 Norma 的显示器，应提示可能产生镜厅效果，并允许取消。

## 8. 状态模型

### 8.1 Workspace Mode

```text
Canvas ←→ LiveReview
```

切换是可逆的，不创建或销毁 Workspace 核心状态。

### 8.2 Live Source

```text
Disconnected
Connecting
Live
Paused
PermissionRequired
Failed
```

### 8.3 Context Brush

```text
Idle
Drawing
Capturing
Ready
Failed
```

`Capturing` 必须可取消。切换模式、切换来源或关闭 Workspace 时，未完成的 capture task 不得写入孤儿资产。

### 8.4 Context 生命周期

```text
Draft → Ready → Attached → Dispatched
   └──────────────→ Removed
```

- `Draft`：正在圈选，未落盘。
- `Ready`：已有可预览的 Context Asset。
- `Attached`：已附加到当前对话，但未发送。
- `Dispatched`：已与一个或多个 Agent Task 建立关联。
- `Removed`：用户移除或资产被清理。

## 9. 性能与线程边界

必须保持的性能不变量：

- 捕获帧推进在后台执行。
- GPUI 前台只更新状态和提交 surface。
- 指针移动热路径不写 SQLite、不截图、不扫描文件。
- 实时画面沿用 24 FPS、队列深度 2、最长边 1920px 的初始预算，除非测量证明需要调整。
- 旧帧被丢弃，不能排队追赶。
- Context 快照、PNG 编码、hash 和磁盘写入在后台执行。
- `Live Region` 只在发出任务时重新捕获，不进行持续图像落盘。
- 模式切换不能阻塞等待 recognizer、PTY、网络或文件 IO。

## 10. 隐私、安全与用户控制

- 捕获外部来源前必须由用户主动选择。
- Live Review 始终显示来源名称和捕获状态。
- 每个 Context Chip 都可以在发送前移除。
- 发送前明确显示将共享给哪些 Agent。
- 不默认广播给所有 Agent。
- Context Manifest 和日志不得包含图片二进制。
- 重新打开项目后，不自动恢复外部窗口捕获，需用户重新确认。
- 外部窗口捕获在首版是只读的，不注入输入事件。
- Context 资产必须限制在当前 Workspace 的 `.multisoul/context-assets` 下，路径经过规范化，不能由 Agent 构造任意写入位置。

## 11. 分阶段实施

### Slice 0：模式骨架与输入安全

范围：

- 顶部 segmented Tab。
- `WorkspacePresentationMode`。
- Live Review 空状态。
- 切换时隐藏原生 Browser View、释放 first responder、清理临时交互。

复杂度：低到中。

通过条件：反复切换不会丢失 Canvas 节点，不会把 Live Review 输入发送给隐藏 Terminal。

### Slice 1：Workspace 内来源与 Crew Read Model

范围：

- 一个 Browser / Preview 或 Ego 来源。
- 中央 Live Surface。
- 紧凑 Crew 状态列表。
- 沿用低拷贝帧路径。

复杂度：中。

通过条件：实时画面稳定，Agent 状态与现有 Coordinator 一致，不改变节点 bounds 或 PTY 尺寸。

### Slice 2：通用 macOS 窗口来源

范围：

- 窗口枚举与来源选择。
- 权限状态和恢复路径。
- iPhone Simulator 窗口验证。
- 系统版本 gate。

复杂度：中到高。

通过条件：允许、拒绝、撤销权限均有明确状态；窗口关闭后能恢复选择，不泄漏捕获 session。

### Slice 3：矩形 Context Brush

范围：

- 矩形框选与 overlay。
- 归一化坐标。
- 后台 snapshot/crop。
- Context Chip 和资产存储。

复杂度：中。

通过条件：Retina、窗口缩放和 letterbox 场景下，截图内容仍与用户框选区域一致。

### Slice 4：多 Agent 视觉上下文分派

范围：

- typed `VisualContextRef`。
- Agent Task 关联。
- Claude/Codex 图片输入与通用路径 fallback。
- 指定目标 Agent 和分派可见性。

复杂度：中到高。

通过条件：两个目标 Agent 获得同一个 Context ID；未选择的 Agent 不获得图片或路径。

## 12. 验收标准与证据

### 12.1 纯逻辑单元测试

至少覆盖：

- `Canvas → LiveReview → Canvas` 不修改 `Document` 节点、bounds 和 viewport。
- 归一化矩形与实际内容矩形之间的双向映射。
- letterbox 区域不会被包含进 Context crop。
- 小于最小尺寸的选区被拒绝。
- `Snapshot` 保持不可变；`LiveRegion` 在下一次发送时生成新快照。
- Context 只附加给明确目标 Agent。
- 多 Agent 分派使用同一 Context ID。
- 路径始终位于 `.multisoul/context-assets` 下。

新增测试必须遵循项目要求：完整场景注释、数据推导、执行过程、正负断言，以及每个断言的失败说明。

### 12.2 Headless GPUI / E2E

至少覆盖：

- 顶部两个 Tab 的 selected、hover、focus 与 disabled 状态。
- Tab 可点击，周围空白仍可拖动窗口。
- Live Review 输入不会路由到隐藏 Terminal。
- 切换模式后 Browser 原生子视图不会覆盖 Live Review。
- 完成一次框选只生成一个 Context Asset。
- 移除 Context 后发送任务不会附加该 Context。
- Crew read model 状态与 Agent Coordinator 一致。

### 12.3 真实 App L3 / Computer Use

页面、布局和视觉状态必须通过 `computer-use:computer-use` 操作真实 App 验收，不能只依赖静态截图或 headless 测试。

真实 App 验收矩阵：

- macOS 权限：未请求、允许、拒绝、随后在 Settings 中允许。
- 来源：Workspace Browser、Ego、普通桌面窗口、iPhone Simulator。
- 显示：Retina 与非整数缩放、窗口 resize、来源 resize、来源关闭。
- 输入：Terminal focus 后切换模式、Browser first responder、中文 IME、语音 composer。
- 性能：持续预览时交互无明显掉帧，模式切换不阻塞。
- 多 Agent：选择两个 Agent 发送同一个 Context，验证两个任务均包含相同引用，第三个未选择 Agent 不包含引用。

### 12.4 性能护栏

需要扩展或新增机械化检查，防止：

- live frame 经 CPU RGBA / `RenderImage` / `img()` 路径上屏。
- `on_mouse_move` 或 runtime 输入热路径执行同步文件 IO。
- 每一帧写 SQLite 或写 snapshot。
- 切换模式时销毁并重建全部 PTY 或 Agent session。

## 13. 主要风险与缓解措施

| 风险 | 后果 | 缓解 |
|---|---|---|
| 把新模式建成第二个 `AppScreen` | 输入、语音、Agent 循环被错误禁用 | 使用 `WorkspacePresentationMode` |
| 原生 WebView 未隐藏 | WebView 浮在 Live Review 之上 | 切换时显式 release/hide/retain visible set |
| Live Review 重新布局现有节点 | 切回 Canvas 后布局被破坏，Terminal 被 resize | 只构建 read model，不写 `Document` |
| 实时帧退回 CPU 图片路径 | 高 CPU、卡顿和内存抖动 | 复用 420f + IOSurface + `surface()` |
| 用户以为所有 Agent 都看到了图片 | 任务理解不一致 | Context Chip 明示目标和传输方式 |
| 文本模型无法原生读取图片 | Agent 只看到路径 | 能力探测；CLI 图片粘贴或路径 fallback；UI 如实标注 |
| `Live Region` 被理解为语义追踪 | 页面变化后截错区域 | 文案说明按归一化区域重截；变更时请求重新确认 |
| 权限/版本兼容不清晰 | 功能在部分 macOS 上无反馈失败 | 显式 version gate、权限状态和 fallback |
| Context 资产无限增长 | Workspace 膨胀 | 生命周期关联、手动清理和后续保留策略 |

## 14. 实施前需要确认的产品决策

本 SPEC 给出默认建议，以下决策不阻塞技术 spike，但应在进入完整实现前确认：

| 决策 | 默认建议 |
|---|---|
| 新模式名称 | `Live Review` |
| 打开 Workspace 的默认模式 | `Canvas` |
| Context 捕获默认策略 | `Snapshot` |
| 框选后是否立即发送 | 否，先生成可移除的 Context Chip |
| 默认共享对象 | Norma；其他 Agent 必须显式选择或分派 |
| Crew 展示形态 | 紧凑 read model，不复制完整 Terminal 卡片 |
| 外部 App 是否可直接操作 | 首版只读 |
| macOS 最低版本 | 暂不改变；12.0–12.2 禁用外部捕获，14+ 优先系统 picker |
| Context 资产保留 | 跟随任务/项目，提供手动清理；自动期限后定 |

## 15. 推荐技术 Spike

在进入完整功能开发前，建议先完成一个可丢弃的纵向 spike：

1. 在现有顶栏加入 `Canvas | Live Review`。
2. Live Review 只接入一个现有 Ego 或 Browser 来源。
3. 完成一个矩形 Context Brush，并生成一张 snapshot。
4. 把同一个 Context ID 和路径分派给两个测试 Agent。
5. 使用真实 App 验证模式切换、原生 View 隐藏、输入隔离、Retina 裁剪和帧性能。

Go 条件：

- 切换模式不破坏 Canvas 状态。
- 隐藏 Terminal 不接收输入。
- live frame 保持低拷贝路径。
- Context crop 与用户框选一致。
- 目标 Agent 获得相同、可追踪的 Context，非目标 Agent 不获得。

No-Go / 重新设计条件：

- 必须复制 Runtime 或节点才能呈现 Live Review。
- 外部捕获只能通过逐帧 CPU 图片转换满足 UI。
- 无法保证原生 Browser View 的 focus 和可见性隔离。
- Context 无法确定性地进入 Agent 任务，只能依赖 prompt 提醒模型。

## 16. 最终推荐

值得做，但应把它作为“同一 Workspace 的协作观察模式”，而不是“第二张画布”。

推荐按以下架构落地：

```text
同一个 Workspace Core
  ├─ Canvas Projection
  └─ Live Review Projection
       ├─ Live Surface Source Adapters
       ├─ Context Brush
       ├─ Crew Read Model
       └─ Agent Visual Context Attachment
```

这个切分保留了 Norma OS 现有 Live Node on Infinite Canvas 的核心，同时补上 vibe coding 最关键的闭环：用户不仅能指挥多个 Agent，还能围绕真实运行结果，直接指出问题、共享上下文并验证改动。
