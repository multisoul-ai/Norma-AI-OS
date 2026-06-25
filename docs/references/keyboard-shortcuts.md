# Keyboard Shortcuts Reference

本文汇总 Multisoul Desktop 当前已注册的快捷键与键盘/指针交互契约。macOS 上 `Cmd` 对应 GPUI 的 `platform` 修饰键（⌘）。

最后对照源码：`crates/app/src/main.rs`、`crates/app/src/app/keyboard.rs`、`crates/app/src/canvas/view.rs`（2026-06-25）。

## 1. 注册机制

快捷键分三层，不要混为一谈：

| 层 | 注册位置 | 机制 | 典型用途 |
| --- | --- | --- | --- |
| **GPUI Action 绑定** | `main.rs` → `cx.bind_keys([...])` | 全局 `KeyBinding` → `actions!` 定义的 Action | 新建卡片、布局、复制、打开项目 |
| **Action 处理器** | `app/mod.rs` → `.on_action(cx.listener(...))` | Action 触发时执行副作用 | 与菜单项共用同一 Action |
| **原始键事件路由** | `app/keyboard.rs` → `on_key_down` / `on_key_up` | 不经 Action 系统，直接读 `Keystroke` | 语音长按、方向键导航、Cmd+V 粘贴、PTY 输入 |

鼠标/滚轮交互不走 Action 系统，由 `canvas/view.rs` 与 `canvas/scroll.rs` 处理。

```text
cx.bind_keys (main.rs)
  → GPUI 分发 Action
  → MultisoulApp::on_* (mod.rs)

on_key_down (keyboard.rs)
  → 语音 / 导航 / 粘贴 / PTY 编码

canvas/view.rs
  → pan / 拖拽 / 选中 / 文本选区 / 滚轮路由
```

## 2. 全局快捷键（GPUI Action）

在 `crates/app/src/main.rs` 通过 `cx.bind_keys` 注册。Action 定义见 `crates/app/src/app/mod.rs` 的 `actions!(multisoul, [...])`。

| 快捷键 | Action | 处理器 | 生效界面 | 行为 |
| --- | --- | --- | --- | --- |
| `⌘N` | `NewTerminal` | `on_new_terminal` | Canvas | 在可见画布区域新建 Terminal 节点，激活选中并触发当前布局 reflow |
| `⌘C` | `CopyCardContent` | `on_copy_card_content` | Canvas | 复制当前终端文本选区到剪贴板；无选区时不写入 |
| `⌘O` | `OpenProject` | `on_open_project` | 全局 | 弹出目录选择器打开 workspace（Launchpad / Canvas 均可用） |
| `⌘⇧O` | `OpenPreviewFile` | `on_open_preview_file` | Canvas | 弹出文件选择器，创建 Preview 卡片 |
| `⌘M` | `ToggleMasterLayout` | `on_toggle_master_layout` | Canvas | 切换 master/detail 布局；无选中时忽略 |
| `⌘⇧L` | `ReflowLayout` | `on_reflow_layout` | Canvas | 退出 master 模式并按最多三列平铺网格重排全部节点 |
| `Esc` | `ExitFocus` | `on_exit_focus` | 全局 | 从 `NodeFocused` 退回 `CanvasSelect`（画布选择模式） |

### 复制（`⌘C`）优先级

`on_copy_card_content` 的复制来源逻辑（`keyboard.rs`）：

1. 若节点已聚焦（`NodeFocused`）且该节点仍存在 → 以该节点为准（即使画布上选中了别的卡片）。
2. 否则若画布上**恰好选中一个**节点 → 以该节点为准。
3. 仅当存在**非空**终端文本选区（`active_text_selection`）时才写入剪贴板。

### Canvas 前置条件

以下 Action 在 `AppScreen::Launchpad` 下会直接返回或设置错误提示，不执行画布操作：

- `NewTerminal`、`OpenPreviewFile`、`ToggleMasterLayout`、`ReflowLayout`
- `CopyCardContent`（静默忽略）

`OpenProject`、`ExitFocus` 不受 Canvas 限制。

## 3. 菜单项（无快捷键绑定）

`main.rs` 的 `Multisoul` 菜单注册了以下 Action，但**没有**对应的 `KeyBinding`：

| 菜单项 | Action | 处理器 | 等效入口 |
| --- | --- | --- | --- |
| New Claude Agent | `NewClaudeAgent` | `on_new_claude_agent` | 顶部工具栏「Claude Code」 |
| New Codex Agent | `NewCodexAgent` | `on_new_codex_agent` | 顶部工具栏「Codex」 |

其余菜单项均有快捷键，见上一节。

## 4. 键事件路由（非 Action）

在 `crates/app/src/app/keyboard.rs` 的 `on_key_down` / `on_key_up` / `on_modifiers_changed` 中处理，不经过 `cx.bind_keys`。

### 4.1 语音输入

| 快捷键 | 阶段 | 行为 |
| --- | --- | --- |
| `⌘⇧V` 按下 | `on_key_down` | 开始语音录音（`VoiceTrigger::Shortcut`） |
| `⌘⇧V` 松开 | `on_key_up` | 结束录音并进入转写 |
| 松开 `⌘` 或 `⇧`（录音仍由 Shortcut 触发） | `on_modifiers_changed` | 结束录音 |

实现：`crates/app/src/app/voice/control.rs` → `is_voice_hold_shortcut`。

**冲突隔离：**

- `⌘V`（粘贴）不匹配语音快捷键。
- `⌘⇧N` 等其它 platform 组合不匹配语音快捷键。

### 4.2 粘贴（`⌘V`）

不经 GPUI Action，在 `on_key_down` 中读取剪贴板并发送 `NodeInput::Text` 到 PTY。

| 焦点状态 | 触发条件 | 行为 |
| --- | --- | --- |
| `NodeFocused` | 始终 | 粘贴剪贴板 UTF-8 文本到聚焦节点 PTY |
| `CanvasSelect` | 方向键刚选中可聚焦卡片（`keyboard_selected_node`）且该卡片为 terminal/agent | 先自动聚焦，再粘贴 |

`⌘V` 不会作为裸字符 `v` 传入 PTY（见 `keystroke_to_input` 对 `modifiers.platform` 的拦截）。

### 4.3 画布卡片导航（方向键）

| 快捷键 | 前置条件 | 行为 |
| --- | --- | --- |
| `↑` `↓` `←` `→` | `CanvasSelect`，且无修饰键 | 在可见区域内按空间关系切换到相邻卡片；更新 `keyboard_selected_node` |

实现：`canvas::navigate_card`（`crates/canvas/src/navigation.rs`）。

### 4.4 键盘选中后自动聚焦

方向键选中 terminal/agent 后，若接下来按下「内容键」，会自动 `focus_node` 并将输入发往 PTY：

- **会触发自动聚焦：** 可打印字符、IME 提交路径、`⌘V` 粘贴
- **不会触发：** 方向键、无修饰 `Space`（留给画布 pan）、`Esc`、`⌘C` 等平台快捷键、Ctrl/Fn 组合

判定函数：`should_auto_focus_keyboard_selection`（`keyboard.rs`）。

### 4.5 空格键（画布 pan）

| 事件 | 行为 |
| --- | --- |
| `Space` 按下 | `CanvasSelect` 下设置 `interaction.space_held = true` |
| `Space` 松开 | 清除 `space_held` |

配合鼠标：`Space` + 左键拖拽 → 平移画布（见 §5）。

### 4.6 节点聚焦时的 PTY 输入

`NodeFocused` 下，除被 app 拦截的平台快捷键外，按键经 `keystroke_to_input`（`cards/term/view.rs`）编码为 `NodeInput::Key` 写入 PTY。

**不会传入 PTY：**

- 任意 `⌘` 组合（`modifiers.platform == true` 时 `keystroke_to_input` 返回 `None`）
- 需交给 IME 的可打印键（`ime::should_defer_keystroke_to_ime`）
- 被 app Action 消费的键（如 `⌘N`、`⌘M`、`Esc` 等）

**会传入 PTY：**

- 字母、数字、符号（含 IME  finalized 字符的 `key_char` 路径）
- `Enter`、`Tab`、`Backspace`、`Esc`、方向键
- `Ctrl+*`、`Alt+*` 组合（如 `Ctrl+C` → ETX）

## 5. 指针与滚轮交互

不走快捷键注册，但属于用户输入契约。实现：`crates/app/src/canvas/view.rs`、`crates/app/src/canvas/scroll.rs`。

### 5.1 鼠标

| 操作 | 行为 |
| --- | --- |
| 单击标题栏 | 选中节点；按住可拖拽移动 |
| 单击空白 | 清空选择，退出节点聚焦 |
| 单击 terminal/agent 正文 | 聚焦节点并开始文本选区 |
| 双击可聚焦节点 | 聚焦（经 `runtime::route`） |
| 中键拖拽 | 平移画布 |
| `Space` + 左键拖拽 | 平移画布 |

### 5.2 滚轮路由

滚轮目标由光标位置、选中状态、卡片类型共同决定（`wheel_scroll_owner_for_point`）：

| 条件 | 滚轮效果 |
| --- | --- |
| 未命中节点，或节点未选中，或命中标题栏 | 画布层（见下） |
| 已选中 Terminal / Agent 正文 | 终端 scrollback 滚动 |
| 已选中 Preview 正文 | GPUI 原生元素滚动（`NativeElement`） |
| Rect 等默认卡片 | 画布层 |

**画布层滚轮**（`apply_canvas_wheel_scroll`）：

- 默认**关闭**（`InteractionState.canvas_scroll_enabled = false`）。
- 用户需点击顶部工具栏 `↕` 按钮开启后，滚轮才平移 `viewport.pan`。
- 当前实现**不会**通过滚轮改变 `viewport.zoom`（旧 zoom 行为已移除，见 `canvas_view_tests.rs`）。

## 6. 输入模式速查

| 模式 | 进入方式 | 退出方式 | 键盘主要行为 |
| --- | --- | --- | --- |
| `CanvasSelect` | 默认；单击选中；Esc | 双击/正文点击 → 聚焦 | 方向键导航；Space 预备 pan；平台快捷键走 Action |
| `NodeFocused` | 双击；terminal 正文点击；键盘自动聚焦 | Esc → `ExitFocus` | 按键进 PTY；`⌘V` 粘贴；`⌘C` 复制选区 |

路由决策的纯函数：`crates/runtime/src/route.rs`。

## 7. 预留 / 未实现

| 项目 | 状态 |
| --- | --- |
| Settings 面板侧栏「Keyboard Shortcuts」 | UI 占位，侧栏项 `enabled = false`，尚无设置页内容（`settings/panel.rs`） |
| New Claude / Codex Agent 快捷键 | 仅菜单与工具栏，无 `KeyBinding` |
| 滚轮缩放 | 文档旧描述（`ARCHITECTURE.md` §5）已过时；当前滚轮不缩放 |
| Settings 面板开关 | 仅工具栏 ⚙ 点击，无快捷键 |

## 8. 源码索引

| 关注点 | 文件 |
| --- | --- |
| `cx.bind_keys` 注册 | `crates/app/src/main.rs` |
| Action 定义与 `on_action` 挂载 | `crates/app/src/app/mod.rs` |
| 键事件 / 粘贴 / 导航 / 复制 | `crates/app/src/app/keyboard.rs` |
| 语音长按快捷键 | `crates/app/src/app/voice/control.rs` |
| GPUI → PTY 编码 | `crates/app/src/cards/term/view.rs` → `keystroke_to_input` |
| IME  defer 判定 | `crates/app/src/app/ime.rs` |
| 画布 pan / 选中 / 文本选区 | `crates/app/src/canvas/view.rs` |
| 滚轮路由 | `crates/app/src/canvas/scroll.rs` |
| 纯路由策略 | `crates/runtime/src/route.rs` |
| 方向键空间导航 | `crates/canvas/src/navigation.rs` |

## 9. 新增快捷键检查清单

1. 在 `actions!` 中声明 Action（若需菜单复用）。
2. 在 `main.rs` 的 `cx.bind_keys` 注册 `KeyBinding`；如需菜单项，同步 `MenuItem::action`。
3. 在 `MultisoulApp::render` 根元素 `.on_action(...)` 挂载处理器。
4. 若快捷键不应传入 PTY，确认 `keystroke_to_input` 对 `modifiers.platform` 的拦截已覆盖；必要时补测试（参考 `test_keystroke_to_input_platform_shortcut_is_not_sent_to_pty`）。
5. 若与 `on_key_down` 自定义路由冲突（如 `⌘V` / `⌘⇧V`），在 `keyboard.rs` 或 `voice/control.rs` 明确优先级并补单元测试。
6. 更新本文档。
