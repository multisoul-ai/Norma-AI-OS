# Norma OS Architecture

Norma OS 是一个 macOS 空间工作台脚手架。当前垂直切片围绕 **Live Node on Infinite Canvas** 构建：GPUI 窗口中有无限画布，画布节点可以承载实时运行的能力；Terminal 是第一个 Live Node。

本文描述当前实现，不描述未来愿景的完整形态。Phase 占位见 [`README.md`](README.md)。

## 1. 系统拓扑

```text
macOS Window（GPUI + Metal）
└── app：窗口、渲染、快捷键、后台副作用编排
    ├── ui：主题与 GPUI 共享组件
    ├── runtime：LiveNode 生命周期、焦点与输入路由
    │   ├── canvas：几何、节点、选择与命中
    │   ├── terminal：PTY、ANSI grid 与输入编码 ──> /bin/zsh
    │   └── storage：SQLite store 与 migration ──> workspace 元数据
    ├── agent：Dialogue/CardTask、持久 Agent 协调规则与高层工具
    ├── asr：模型 ID、受控目录、清单、哈希与原子安装
    └── voice_sherpa：Sherpa recognizer、VAD、PCM partial/final adapter
        ├── asr：消费已校验的 AvailableAsrModel
        ├── dialogue：消费引擎中立的语音协议
        └── audio：消费共享 PCM 类型
```

核心 Cargo 依赖方向：

```text
app -> {ui, runtime, canvas, terminal, storage, agent, asr, voice_sherpa}
voice_sherpa -> {asr, dialogue, audio}
runtime -> {canvas, terminal, storage}
storage -> {agent, canvas, workspace, rusqlite}
agent -> norma
ui -> gpui
canvas -> {serde, thiserror}
terminal -> {portable-pty, alacritty_terminal, crossbeam-channel}
asr -> {serde, sha2, tar, bzip2, tempfile}
```

`agent`、`asr`、`canvas`、`terminal`、`runtime`、`storage` 是零 GPUI 层；`agent` 还保持零 Storage 依赖，`asr` 保持零 Sherpa/网络依赖。识别器适配归 `voice_sherpa`，下载和 Runtime/Storage 副作用编排归 `app`。`storage` 不得依赖 `app/runtime/ui`，`canvas/terminal` 不得依赖 `storage`。`app` 和 `ui` 是壳层/渲染层，可以使用 GPUI。

## 2. 代码结构

### `crates/asr/`

零 GPUI/Sherpa/网络的 ASR 领域层，负责不可变模型 ID、受控目录、安装清单、SHA-256、
安全解压、原子安装和共享 Silero VAD 支持工件。Sherpa 的 Paraformer/Transducer/Zipformer2 CTC 分支只存在于
`crates/voice_sherpa/`；Settings 下载、进度、选择和删除编排只存在于 `crates/app/`。

### `crates/agent/`

零 GPUI/Storage 的 Agent 领域层，负责 Dialogue、CardTask、Provider 工具循环、Agent 呼叫名、
动态名册和按定义隔离的 FIFO 协调规则。它只计算高层命令与结构化回执；卡片激活、PTY 输入和
SQLite journal 由 `app` adapters 执行。

### `crates/canvas/`

零 GPUI 域层，负责无限画布的纯数据模型与几何计算。

```text
crates/canvas/src/
├── lib.rs          # 对外导出 Document、Viewport、Node、Selection 等
├── geometry.rs     # Vec2、CanvasRect、Viewport、Transform2D
├── document.rs     # 节点集合、viewport、id/z-index 分配、节点移动
├── node.rs         # CanvasNode、NodeKind/CanvasNodeKind、NodeType、TermSpec、AgentSpec
├── selection.rs    # BTreeSet 选择集
├── hit_test.rs     # 画布坐标命中测试，返回最高 z-index 节点
├── commands.rs     # Phase 2: undo/redo command model 占位
└── history.rs      # Phase 2: undo/redo history stack 占位
```

主要职责：

- 维护画布坐标和屏幕坐标的转换规则：`screen = canvas * zoom + pan`
- 管理节点 id、bounds、z-index、selection
- 提供不依赖 UI 的命中测试与移动语义

### `crates/terminal/`

零 GPUI 域层，负责真实 PTY、ANSI 解析、终端输入编码和 session 状态。

```text
crates/terminal/src/
├── lib.rs          # 对外导出 TerminalSession、TerminalGrid、KeyInput 等
├── pty.rs          # portable-pty 封装：spawn/read thread/write/resize/kill
├── grid.rs         # alacritty_terminal 封装，输出中性 GridSnapshot
├── grid/links.rs   # OSC-8、裸 http(s) URL 与安全相对预览路径识别
├── input.rs        # KeyInput -> PTY bytes
├── session.rs      # PtyHandle + TerminalGrid + channel receiver
└── message.rs      # PtyEvent::Output / Exited
```

主要职责：

- 启动 shell 子进程并桥接 PTY 输出
- 将原始字节交给 `alacritty_terminal` 解析为屏幕网格
- 将 GPUI 层翻译后的按键编码为 PTY 字节
- 维护 `Running` / `Exited` 状态和 resize 后的行列数

### `crates/storage/`

零 GPUI 存储层，负责 SQLite store、migration 和持久化边界。

```text
crates/storage/src/
├── app_store.rs       # 用户级 app.sqlite
├── project_store.rs   # workspace/.multisoul/project.sqlite
├── migrations.rs      # app/project schema migration
├── recent_store.rs    # 最近项目
├── settings_store.rs  # 全局设置
├── canvas_store.rs    # DocumentSnapshot
├── session_store.rs   # session checkpoint（按 node_id 覆盖）
├── agent_definition_store.rs # 项目级持久 Agent 定义
└── agent_dispatch_store.rs   # 委派任务 journal 与状态转换
```

主要职责：

- 管理用户级 `~/Library/Application Support/Norma OS/app.sqlite`
- 管理项目级 `<workspace>/.multisoul/project.sqlite`
- 读写 recent/settings/canvas snapshot/session checkpoint、Agent 定义与委派 journal
- 保持底层依赖，不反向依赖 `app/runtime/ui`

热路径禁令：`Runtime::drain_all`、PTY output chunk、`canvas_view::on_mouse_move`、`runtime::route`、`send_node_input` 不得同步写 SQLite 或阻塞查库。

### `crates/runtime/`

零 GPUI 域层，负责把 canvas 文档与 live node runtime 组装成可运行状态。

```text
crates/runtime/src/
├── lib.rs
├── live.rs          # LiveNode、Caps、NodeInput、NodeResize、NodeView
├── nodes/
│   ├── mod.rs
│   ├── term.rs      # TermLive：TerminalSession wrapper
│   └── agent.rs     # Agent live factory/wrapper
├── runtime.rs       # Runtime：Document、Selection、FocusState、NodeId -> LiveNode
├── focus.rs         # CanvasSelect / NodeFocused(NodeId)
├── route.rs         # InputMode + InputEvent -> Route
└── snapshot.rs      # Phase 4: snapshot cache / scrollback virtualization 占位
```

主要职责：

- `spawn_terminal`：创建 `TermLive` 并在 `Document` 中加入 `NodeKind::Term` 描述节点
- `spawn_agent`：创建 terminal-backed live node，并在 `Document` 中加入 `NodeKind::Agent` 描述节点
- `drain_all`：批量排空所有 live node 输出，返回是否需要 repaint
- `kill_all`：窗口关闭时清理所有子进程
- 用纯函数表达输入路由，避免把策略锁死在 GPUI 事件回调里

### `crates/ui/`

GPUI-facing 共享 UI 基础。当前只有暗色主题 token。

```text
crates/ui/src/
├── lib.rs
└── theme.rs        # Theme::dark(), 0xRRGGBB tokens
```

### `crates/app/`

GPUI/Metal 壳层，负责窗口、菜单、快捷键、鼠标事件、渲染和副作用执行。

```text
crates/app/src/
├── main.rs          # tracing、GPUI boot、菜单、Cmd+N、Esc
├── assets.rs        # 内嵌 icon / wallpaper
├── app/             # MultisoulApp 壳层、keyboard、IME、voice、chrome
├── canvas/          # pan/zoom/drag、master layout、interaction
├── motion/          # card motion state
├── norma/           # Norma server + transport
├── settings/        # settings panel、hooks UI
├── workspace/       # launchpad、flow、state、persistence
├── preview/         # PreviewStore
└── cards/
    ├── mod.rs       # Card trait + static registry
    ├── chrome.rs    # shared card shell helpers
    ├── term/        # Terminal card
    └── agent/       # Agent card
```

主要职责：

- 启动 GPUI 应用窗口
- 处理 `Cmd+N`、`Esc`、鼠标拖拽、滚轮缩放
- 将用户输入转换为 `runtime::route` 能理解的事件
- 根据 Runtime 状态渲染画布节点；具体卡片内容按 `NodeType -> Card` 分发

## 3. 内部数据模型

### Canvas

核心类型：

- `Document`: `Vec<CanvasNode>`、`Viewport`、递增 `next_id`、递增 `max_z`
- `CanvasNode`: `id`、`kind`、`bounds`、`z_index`、`selected`
- `NodeKind`/`CanvasNodeKind`: `Rect(RectNode)`、`Term(TermSpec)`、`Agent(AgentSpec)`
- `NodeType`: `Rect`、`Term`、`Agent`，供 runtime/app dispatch
- `Selection`: 稳定有序的 `BTreeSet<NodeId>`

坐标约定：

```text
screen_point = canvas_point * viewport.zoom + viewport.pan
canvas_point = (screen_point - viewport.pan) / viewport.zoom
```

节点移动 API 接收画布坐标系 delta。`app` 层从鼠标屏幕位移计算节点移动时必须除以 zoom。

### Terminal

核心类型：

- `PtyHandle`: 持有 master PTY、writer、child process
- `PtyEvent`: `Output(Vec<u8>)` 或 `Exited { code }`
- `TerminalGrid`: 持有 `alacritty_terminal::Term` 和 ANSI `Processor`
- `GridSnapshot`: 中性 rows/cursor/cols/lines 与每个 Cell 的可选 OSC-8 hyperlink，不暴露 alacritty 类型给 UI
- `TerminalSession`: `PtyHandle` + `TerminalGrid` + channel receiver + status

PTY 输出通过 `crossbeam-channel` 从读线程传到 session。`TerminalSession::drain` 将输出批量喂给 grid，并在有变化时返回 `dirty = true`。

`terminal` 只负责从 snapshot 解析可点击事实：OSC-8 和裸文本仅接受 `http://`/`https://`，相对预览路径拒绝绝对路径与 `.`/`..`。是否打开 Browser 卡片、系统浏览器或 Preview 卡片属于 `app` 副作用；点击与文本选择优先级也由 `app/canvas` 决定。

### Runtime

核心类型：

- `Runtime`: `Document`、`Selection`、`FocusState`、`HashMap<NodeId, Box<dyn LiveNode>>`
- `LiveNode`: `caps`、`drain`、`resize`、`input`、`view`、`kill`
- `InputMode`: `CanvasSelect` 或 `NodeFocused(NodeId)`
- `InputEvent`: click、double-click、key、escape
- `Route`: select、clear、focus node、send input、exit focus、ignore

`runtime` 不知道 GPUI 事件，也不渲染任何像素；它只维护产品状态和可测试决策。

### Card RuntimeEvent 扩展契约

后续卡片状态扩展应通过统一的 `RuntimeEvent` 契约表达，而不是让每个 card 自己发散状态来源。事件层分两类：

- **进程运行状态**：来自 `LiveNode` / `TerminalSession`，用于描述卡片底层进程是否 `Running`、`Finished`、`Error` 或 `Stopped`。
- **Agent 任务状态**：来自 Claude、Codex、Cursor、OpenCode hooks，描述 Agent 当前是否 `Thinking`、`RunningTool`、`WaitingApproval`、`Done` 或 `Failed`。

事件消费采用 `drain_all()` 后 `take_events()` 的拉取模型；hook 事件先进入 app 层的本地 ingest，再归一化为卡片事件。不得把 PTY output chunk 当成 `RuntimeEvent`，不得在 `Runtime::drain_all`、hook ingest、PTY output 热路径中同步写 SQLite。

详细开发指南见 [`docs/references/card-runtime-events.md`](docs/references/card-runtime-events.md)。

## 4. 核心流程

### 新建 Terminal 节点

```text
用户按 Cmd+N
  ↓
app::MultisoulApp::on_new_terminal
  ↓
Runtime::spawn_terminal(bounds, "/bin/zsh", 80, 24)
  ↓
TermLive::spawn(session_id, shell, cols, rows)
  ↓
PtyHandle::spawn 启动 shell 子进程和 PTY 读线程
  ↓
Document::add_node(NodeKind::Term(TermSpec), bounds)
  ↓
Runtime.live.insert(node_id, TermLive)
  ↓
Selection::select_only(node_id)
  ↓
cx.notify() 触发重绘
```

### PTY 输出到屏幕

```text
shell 写入 PTY
  ↓
pty-reader thread 读到 bytes
  ↓
crossbeam channel 发送 PtyEvent::Output(bytes)
  ↓
app 背景循环每约 12ms 调 Runtime::drain_all
  ↓
TermLive::drain → TerminalSession::drain
  ↓
TerminalGrid::advance(bytes)
  ↓
dirty=true 时 cx.notify()
  ↓
cards/term 读取 NodeView::Term(GridSnapshot) 并逐行渲染
```

### 键盘输入写入 Terminal

```text
用户双击 Terminal 节点
  ↓
runtime::route(CanvasSelect, DoubleClick{is_focusable=true})
  ↓
FocusState::focus_node(node_id)
  ↓
用户按键
  ↓
app::cards::term::view::keystroke_to_input
  ↓
runtime::route(NodeFocused(node_id), Key)
  ↓
Runtime::send_node_input(node_id, NodeInput::Key)
  ↓
TermLive::input → TerminalSession::write_input
  ↓
terminal::input::encode
  ↓
PtyHandle::write(bytes)
```

`Esc` 由 GPUI key binding 触发 `ExitFocus` action，调用 `FocusState::exit_to_canvas`。

### 画布 pan / zoom / drag

```text
space+left-drag 或 middle-drag
  ↓
app 记录 last_screen
  ↓
MouseMove 屏幕 delta 直接加到 Viewport.pan
```

```text
wheel scroll
  ↓
cursor screen point -> canvas point
  ↓
更新 zoom，并重新计算 pan
  ↓
保持光标下的 canvas point 不动
```

```text
left-click title bar + drag
  ↓
hit_test 找到节点
  ↓
screen delta / zoom -> canvas delta
  ↓
Document::move_node(node_id, canvas_delta)
```

### 窗口关闭与子进程清理

```text
MultisoulApp drop
  ↓
Runtime::kill_all
  ↓
TerminalSession::kill
  ↓
PtyHandle::kill
  ↓
child.kill() + child.wait()
```

`PtyHandle::Drop` 也会调用 `kill`，作为释放路径的兜底。

## 5. 交互契约

| 操作 | 当前行为 |
|------|----------|
| `Cmd+N` | 新建 Terminal 节点，并将全部节点按最多三列的平铺网格重排 |
| 单击节点 | 选中节点并退出终端聚焦 |
| 单击空白 | 清空选择并退出终端聚焦 |
| 双击 Terminal | 聚焦该终端，后续按键写入 PTY |
| `Esc` | 退出终端聚焦 |
| 标题栏拖拽 | 移动节点 |
| 空格+左键拖拽 | pan 画布 |
| 中键拖拽 | pan 画布 |
| 滚轮 | 以光标为锚点缩放 |

## 6. 非目标与边界

当前切片不包含：

- 多窗口/多工作区管理
- 终端 scrollback 虚拟化
- 复制粘贴、mouse reporting、完整 resize UI
- 远程同步、移动端控制、REST/WS API
- codesign / notarization

这些能力在 README 的 Phase 映射中保留扩展位置。

## 7. 技术栈

- Rust 2021 workspace
- GPUI `=0.2.2`
- portable-pty `0.9`
- alacritty_terminal `=0.26`
- crossbeam-channel `0.5`
- tracing / tracing-subscriber
- rusqlite bundled SQLite
- cargo-packager
- macOS Metal toolchain

## 8. 验证与质量闸

常用验证：

```bash
cargo test -p canvas -p terminal -p runtime -p storage
cargo test --workspace
cargo check -p app
cargo build -p app --release
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
bash scripts/check-storage-boundaries.sh
```

`cargo tree ... grep -i gpui` 应无输出，用来确认域层没有引入 GPUI；`check-storage-boundaries.sh` 用来确认存储层依赖方向和热路径禁令。

测试规范见 [`CLAUDE.md`](CLAUDE.md) 第 7 节：测试需要写清场景、数据构造、执行过程、预期结果；关键断言必须有正/负验收与失败说明。
