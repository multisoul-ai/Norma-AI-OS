# Multisoul Desktop Architecture

Multisoul Desktop 是一个 macOS 空间工作台脚手架。当前垂直切片围绕 **Live Node on Infinite Canvas** 构建：GPUI 窗口中有无限画布，画布节点可以承载实时运行的能力；Terminal 是第一个 Live Node。

本文描述当前实现，不描述未来愿景的完整形态。Phase 占位见 [`README.md`](README.md)。

## 1. 系统拓扑

```text
┌──────────────────────────────────────────────┐
│ macOS Window                                  │
│ GPUI + Metal                                  │
│ menu / key binding / mouse events             │
└──────────────────────┬───────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────┐
│ app crate                                     │
│ MultisoulApp                                  │
│ canvas_view / terminal_view                   │
└───────────────┬───────────────────┬──────────┘
                │                   │
                ▼                   ▼
┌──────────────────────────┐   ┌──────────────────────────┐
│ runtime crate             │   │ ui crate                  │
│ document + selection      │   │ theme tokens              │
│ focus + event routing     │   │ GPUI-facing primitives    │
│ terminal session registry │   └──────────────────────────┘
└──────────────┬───────────┘
               │
       ┌───────┴────────┐
       ▼                ▼
┌──────────────┐   ┌──────────────────────────────┐
│ canvas crate │   │ terminal crate                │
│ geometry     │   │ portable-pty + alacritty grid │
│ nodes        │   │ input encoding + sessions     │
│ hit testing  │   └──────────────┬───────────────┘
└──────────────┘                  │
                                  ▼
                         ┌────────────────┐
                         │ /bin/zsh shell │
                         │ child process  │
                         └────────────────┘
```

核心 Cargo 依赖方向：

```text
app -> {ui, runtime, canvas, terminal}
runtime -> {canvas, terminal}
ui -> gpui
canvas -> thiserror
terminal -> {portable-pty, alacritty_terminal, crossbeam-channel}
```

`canvas`、`terminal`、`runtime` 是域层，必须保持零 GPUI；`app` 和 `ui` 是壳层/渲染层，可以使用 GPUI。

## 2. 代码结构

### `crates/canvas/`

零 GPUI 域层，负责无限画布的纯数据模型与几何计算。

```text
crates/canvas/src/
├── lib.rs          # 对外导出 Document、Viewport、Node、Selection 等
├── geometry.rs     # Vec2、CanvasRect、Viewport、Transform2D
├── document.rs     # 节点集合、viewport、id/z-index 分配、节点移动
├── node.rs         # CanvasNode、CanvasNodeKind、RectNode、TerminalNodeRef
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
├── input.rs        # KeyInput -> PTY bytes
├── session.rs      # PtyHandle + TerminalGrid + channel receiver
└── message.rs      # PtyEvent::Output / Exited
```

主要职责：

- 启动 shell 子进程并桥接 PTY 输出
- 将原始字节交给 `alacritty_terminal` 解析为屏幕网格
- 将 GPUI 层翻译后的按键编码为 PTY 字节
- 维护 `Running` / `Exited` 状态和 resize 后的行列数

### `crates/runtime/`

零 GPUI 域层，负责把 canvas 文档与 terminal sessions 组装成可运行状态。

```text
crates/runtime/src/
├── lib.rs
├── runtime.rs       # Runtime：Document、Selection、FocusState、sessions
├── focus.rs         # CanvasSelect / TerminalFocused(NodeId)
├── event_router.rs  # InputMode + InputEvent -> Route
└── snapshot.rs      # Phase 4: snapshot cache / scrollback virtualization 占位
```

主要职责：

- `spawn_terminal`：创建 `TerminalSession` 并在 `Document` 中加入 Terminal 节点
- `drain_all`：批量排空所有 session 的 PTY 输出，返回是否需要 repaint
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
├── app.rs           # MultisoulApp、帧循环、键盘路由、Drop 清理
├── canvas_view.rs   # pan/zoom/drag/click/double-click、viewport culling
└── terminal_view.rs # GridSnapshot 渲染、GPUI Keystroke -> KeyInput
```

主要职责：

- 启动 GPUI 应用窗口
- 处理 `Cmd+N`、`Esc`、鼠标拖拽、滚轮缩放
- 将用户输入转换为 `runtime::route` 能理解的事件
- 根据 Runtime 状态渲染画布节点和终端网格

## 3. 内部数据模型

### Canvas

核心类型：

- `Document`: `Vec<CanvasNode>`、`Viewport`、递增 `next_id`、递增 `max_z`
- `CanvasNode`: `id`、`kind`、`bounds`、`z_index`、`selected`
- `CanvasNodeKind`: `Rect(RectNode)` 或 `Terminal(TerminalNodeRef)`
- `TerminalNodeRef`: 指向 runtime 中的 `session_id`
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
- `GridSnapshot`: 中性 rows/cursor/cols/lines，不暴露 alacritty 类型给 UI
- `TerminalSession`: `PtyHandle` + `TerminalGrid` + channel receiver + status

PTY 输出通过 `crossbeam-channel` 从读线程传到 session。`TerminalSession::drain` 将输出批量喂给 grid，并在有变化时返回 `dirty = true`。

### Runtime

核心类型：

- `Runtime`: `Document`、`Selection`、`FocusState`、`HashMap<TerminalSessionId, TerminalSession>`
- `InputMode`: `CanvasSelect` 或 `TerminalFocused(NodeId)`
- `InputEvent`: click、double-click、key、escape
- `Route`: select、clear、focus terminal、send key、exit focus、ignore

`runtime` 不知道 GPUI 事件，也不渲染任何像素；它只维护产品状态和可测试决策。

## 4. 核心流程

### 新建 Terminal 节点

```text
用户按 Cmd+N
  ↓
app::MultisoulApp::on_new_terminal
  ↓
Runtime::spawn_terminal(bounds, "/bin/zsh", 80, 24)
  ↓
TerminalSession::spawn(session_id, shell, cols, rows)
  ↓
PtyHandle::spawn 启动 shell 子进程和 PTY 读线程
  ↓
Document::add_node(CanvasNodeKind::Terminal(session_id), bounds)
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
TerminalSession::drain
  ↓
TerminalGrid::advance(bytes)
  ↓
dirty=true 时 cx.notify()
  ↓
terminal_view 读取 GridSnapshot 并逐行渲染
```

### 键盘输入写入 Terminal

```text
用户双击 Terminal 节点
  ↓
runtime::route(CanvasSelect, DoubleClick{is_terminal=true})
  ↓
FocusState::focus_terminal(node_id)
  ↓
用户按键
  ↓
app::terminal_view::keystroke_to_input
  ↓
runtime::route(TerminalFocused(node_id), Key)
  ↓
TerminalSession::write_input
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

- 持久化工作区或最近项目
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
- cargo-packager
- macOS Metal toolchain

## 8. 验证与质量闸

常用验证：

```bash
cargo test -p canvas -p terminal -p runtime
cargo test --workspace
cargo check -p app
cargo build -p app --release
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
```

最后一条应无输出，用来确认域层没有引入 GPUI。

测试规范见 [`CLAUDE.md`](CLAUDE.md) 第 7 节：测试需要写清场景、数据构造、执行过程、预期结果；关键断言必须有正/负验收与失败说明。
