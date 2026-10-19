# Keyboard Shortcuts Reference

本文汇总 Norma OS 当前可配置快捷键与键盘/指针交互契约。macOS 上 `Cmd` 对应 GPUI 的 `platform` 修饰键（⌘）。

最后对照源码：`crates/app/src/shortcuts.rs`、`crates/app/src/app/keyboard.rs`、`crates/app/src/settings/keyboard.rs`、`crates/app/src/canvas/view.rs`、`crates/app/src/app/command_dock.rs`（2026-10-10）。

## 1. 注册机制

快捷键分三层，不要混为一谈：

| 层 | 注册位置 | 机制 | 典型用途 |
| --- | --- | --- | --- |
| **App shortcut registry** | `shortcuts.rs` → `default_shortcut_entries()` | 默认快捷键 + SQLite `settings` JSON override → 有效快捷键 | 新建卡片、布局、复制、打开项目、打开 Settings |
| **Action 处理器** | `app/mod.rs` → `.on_action(cx.listener(...))` | Action 触发时执行副作用 | 与菜单项共用同一 Action |
| **原始键事件路由** | `app/keyboard.rs` → `on_key_down` / `on_key_up` | 先处理 Settings 录制态和 registry 命令，再处理手写输入路由 | 语音长按、方向键导航、Cmd+V 粘贴、PTY 输入 |

鼠标/滚轮交互不走 Action 系统，由 `canvas/view.rs` 与 `canvas/scroll.rs` 处理。

```text
shortcuts.rs registry + SettingsStore overrides
  → on_key_down 匹配 Keystroke
  → MultisoulApp::dispatch_shortcut_command

on_key_down (keyboard.rs)
  → Settings 录制 / app 命令 / 语音 / 导航 / 粘贴 / PTY 编码

canvas/view.rs
  → pan / 拖拽 / 选中 / 文本选区 / 滚轮路由
```

## 2. 可配置 App 命令快捷键

默认值在 `crates/app/src/shortcuts.rs` 注册。用户 override 以 JSON 存在 app SQLite 的 `settings` 表，key 为 `keyboard.shortcuts.v1`。Settings → Keyboard Shortcuts 页面会即时更新内存中的有效快捷键；无需重启。

| 默认快捷键 | Command | 处理器 | 生效界面 | 行为 |
| --- | --- | --- | --- | --- |
| `⌘N` | `NewWorkspace` | `navigate_desktop_shortcut` | 桌面主界面 | 打开创建工作空间命名表单 |
| `⌘⇧A` | `AddProject` | `navigate_desktop_shortcut` | 工作空间／项目画布 | 打开当前空间的项目选择器 |
| `⌘K` | `FocusCommandInput` | `focus_command_input` | Canvas | 聚焦底部指令输入框，支持录制及恢复 |
| `⌘⇧N` | `NewWindow` | `on_new_window` | 全局 | 打开新的 Launchpad 窗口，不创建画布节点 |
| `⌘1` | `NewTerminal` | `on_new_terminal` | Canvas | 在可见画布区域新建 Terminal 节点，激活选中并触发当前布局 reflow |
| 未设置 | `NewBrowser` | `on_new_browser` | Canvas | 在可见画布区域创建 Browser 卡片 |
| `⌘C` | `CopyCardContent` | `on_copy_card_content` | Canvas | 复制当前终端文本选区到剪贴板；无选区时不写入 |
| `⌘O` | `OpenProject` | `on_open_project` | 全局 | 弹出目录选择器打开 workspace（Launchpad / Canvas 均可用） |
| `⌘⇧O` | `OpenPreviewFile` | `on_open_preview_file` | Canvas | 弹出文件选择器，创建 Preview 卡片 |
| `⌘S` | `SavePreviewFile` | `save_preview_edit` | 文本 / Markdown 卡片编辑中 | 保存当前编辑；检测到外部修改时保留草稿并提示冲突 |
| `⌘2` | `NewClaudeAgent` | `on_new_claude_agent` | Canvas | 创建 Claude Code agent 卡片 |
| `⌘3` | `NewCodexAgent` | `on_new_codex_agent` | Canvas | 创建 Codex agent 卡片 |
| `⌘4` | `NewCursorAgent` | `on_new_cursor_agent` | Canvas | 创建 Cursor Agent CLI 卡片 |
| `⌘5` | `NewOpenCodeAgent` | `on_new_opencode_agent` | Canvas | 创建 OpenCode Agent CLI 卡片 |
| `⌘6` | `NewGrokAgent` | `on_new_grok_agent` | Canvas | 创建 Grok Build Agent CLI 卡片 |
| `⌘7` | `NewKimiAgent` | `on_new_kimi_agent` | Canvas | 创建 Kimi Code CLI 卡片 |
| 未绑定 | `NewZCodeAgent` | `on_new_zcode_agent` | Canvas | 创建 ZCode CLI 卡片；默认不绑定，可在 Settings 中录制 |
| 未绑定 | `NewInfCodeAgent` | `on_new_infcode_agent` | Canvas | 创建 InfCode CLI 卡片；默认不绑定，可在 Settings 中录制 |
| `⌘8` | `NewPiAgent` | `on_new_pi_agent` | Canvas | 创建 Pi CLI 卡片 |
| `⌘9` | `NewHermesAgent` | `on_new_hermes_agent` | Canvas | 创建 Hermes Agent CLI 卡片 |
| `⌘D` | `DeleteCard` | `on_delete_card_shortcut` | Canvas | 删除当前聚焦卡片；无聚焦时仅删除唯一选中的画布卡片 |
| `⌘M` | `ToggleMasterLayout` | `on_toggle_master_layout` | Canvas | 切换 master/detail 布局；无选中时忽略 |
| `⌘⇧L` | `ReflowLayout` | `on_reflow_layout` | Canvas | 退出 master 模式并按最多三列平铺网格重排全部节点 |
| `⌘Esc` | `ExitFocus` | `on_exit_focus` | 全局 | 从 `NodeFocused` 退回 `CanvasSelect`（画布选择模式） |
| `⌘,` | `ToggleSettings` | `toggle_settings_panel` | Canvas | 打开或关闭 Settings 面板 |
| `⌘E` | `VoiceInteraction` | `activate_ready_voice` | 全局 | 按当前语音交互模式开始/停止单次语音指令或持续聆听；Hold 模式下松开结束输入 |

### 复制（`⌘C`）优先级

文件卡片的编辑输入框聚焦时，复制、粘贴、撤销、方向键及文本选择由输入组件处理，不触发画布操作；`⌘S` 按配置保存文件。

`on_copy_card_content` 的复制来源逻辑（`keyboard.rs`）：

1. 若节点已聚焦（`NodeFocused`）且该节点仍存在 → 以该节点为准（即使画布上选中了别的卡片）。
2. 否则若画布上**恰好选中一个**节点 → 以该节点为准。
3. 仅当存在**非空**终端文本选区（`active_text_selection`）时才写入剪贴板。

### 删除（`⌘D`）目标优先级

`on_delete_card_shortcut` 复用标题栏关闭按钮的 `InputEvent::CloseNode` → `Route::RemoveNode` → `close_node_through_route` 路径：

1. 若节点已聚焦（`NodeFocused`）且该节点仍存在 → 删除该聚焦节点。
2. 否则若画布上**恰好选中一个**仍存在的节点 → 删除该节点。
3. 多选、空选择、失效选择或 Launchpad 下静默忽略。

删除后的布局 reflow、preview 清理、master/detail 更新、交互状态清理、canvas/session dirty 标记与标题栏关闭行为保持一致。

### Canvas 前置条件

以下命令在 `AppScreen::Launchpad` 下会直接返回或设置错误提示，不执行画布操作：

- `NewTerminal`、`NewBrowser`、`OpenPreviewFile`、`NewClaudeAgent`、`NewCodexAgent`、`NewCursorAgent`、`NewOpenCodeAgent`、`NewGrokAgent`、`NewKimiAgent`、`NewZCodeAgent`、`NewInfCodeAgent`、`NewPiAgent`、`NewHermesAgent`、`DeleteCard`、`ToggleMasterLayout`、`ReflowLayout`、`ToggleSettings`
- `CopyCardContent`（静默忽略）

`NewWindow`、`OpenProject`、`ExitFocus` 不受 Canvas 限制。

### Settings 页面校验规则

Settings → Keyboard Shortcuts 支持内联录制、单项 Restore 和 Restore Defaults。校验规则：

- 不允许两个可编辑命令使用同一快捷键（含默认 `⌘E` 的 `VoiceInteraction`）。
- 不允许覆盖只读输入行为：`⌘V`、裸方向键、裸 `Space`。
- 不允许裸可打印字符作为 app 命令快捷键，避免干扰 terminal 输入。
- 录制非 Exit Focus 命令时，裸 `Esc` 取消录制；录制 Exit Focus 时，裸 `Esc` 可作为快捷键。

### 桌面工作空间创建与导航（2026-10-01）

| 默认键位 | 行为 |
| --- | --- |
| ⌘N | 创建工作空间：打开名称表单，确认后创建空空间 |
| ⇧⌘A | 在当前工作空间添加项目：打开最近项目／文件夹／克隆选择器 |
| ←/→/↑/↓（裸方向键，仅项目总览） | 在项目卡片间移动键盘选中态（描边、底色与阴影高亮）：←/→ 按加入顺序循环，首次按下无选中时从首／末个开始；↑/↓ 在两行网格中同列换行（底行缺格吸附到末项，行边界不环绕），首次按下无选中时分别落末项／首项；不展开项目 |
| ⌘← / ⌘→ | 项目总览有选中项目时：进入该选中的项目画布；否则（及已展开项目内）展开当前工作空间的上一个 / 下一个项目（内层，高频） |
| ⌘↓ | 项目总览有选中项目时：展开该选中的项目；无选中（及已展开项目内）时展开当前项目，无当前项目时展开首个项目 |
| ⇧⌘← / ⇧⌘→ | 上一个 / 下一个工作空间（外层，多一个 Shift） |
| ⌘↑ | 返回当前工作空间的项目总览，自动高亮刚展开的项目；返回动画期间即可用方向键移动选择 |
| ⇧⌘↑ | 返回全部工作空间目录 |
| ⇧⌘K | 工作空间快速切换器（已有） |

工作空间按创建顺序、项目按加入顺序循环；同时间戳按 ID 排序。工作空间列表与键盘顺序一致，新建追加末尾，最近访问不再重排列表。
项目切换仅作用于当前工作空间，直接进入对应项目的 Agent 画布并沿用同级切换动画，空列表无操作；单项目不会创建副本。
创建空间可从工作空间列表、项目列表或展开的项目触发；添加项目需先进入一个工作空间，目录页不会把项目加到上次访问的空间。
创建／添加／重命名／删除／克隆／目标选择表单及 Settings 打开期间，桌面导航快捷键不切走背景；先确认或取消表单。
连按以最新待切换目标为基准。嵌入项目的输入交由窗口宿主导航；后台 Agent 保持运行。
所有键位可在 Settings → Keyboard Shortcuts 中录制、恢复；同一窗口中所有项目立即同步。
桌面目录和项目总览也支持 ⌘, 打开设置。裸方向键、Ctrl+方向键与 Cmd+1…6 保持原行为。
键位规律：⌘+方向键作用于项目，加 Shift 上升一层作用于工作空间；←/→ 同级切换，↑/↓ 两行网格换行，↑ 逐级返回，⌘↓ 进入选中项目。
⌘←/→/↓ 在命令输入框、文件编辑器与 Settings 文本框聚焦时仍是行首/行尾/段末编辑键，不触发导航；终端聚焦时 ⌘ 组合本就不进入 PTY，由导航消费。
2026-10-06 起旧键位（⌃⌘←/→/↑、⌥⌘←/→/↑）不再生效；已在 Settings 中自行录制的键位不受影响，未改动的项目随默认值更新。

## 3. 原生菜单栏与创建命令

`lib.rs` 注册 macOS 原生菜单栏；菜单点击、底部指令和可配置快捷键共用既有 Action/处理器，不复制创建逻辑。顶级层级固定为：

```text
Norma OS: Settings… / Services / Quit Norma OS (Cmd+Q)
File:     New ▸ (Window / Cards / Agents) / Open Project…
Edit:     Copy
Layout:   Reflow Layout / Toggle Master Layout
```

`File → New` 的 Agent 命令与数字快捷键共用同一 Action，并可在 Settings 中重新分配：

| 菜单项 | 默认快捷键 | Action | 处理器 | 等效入口 |
| --- | --- | --- | --- | --- |
| New Claude Agent | `⌘2` | `NewClaudeAgent` | `on_new_claude_agent` | `/new claude` |
| New Codex Agent | `⌘3` | `NewCodexAgent` | `on_new_codex_agent` | `/new codex` |
| New Cursor Agent | `⌘4` | `NewCursorAgent` | `on_new_cursor_agent` | `/new cursor` |
| New OpenCode Agent | `⌘5` | `NewOpenCodeAgent` | `on_new_opencode_agent` | `/new opencode` |
| New Grok Agent | `⌘6` | `NewGrokAgent` | `on_new_grok_agent` | `/new grok` |
| New Kimi Agent | `⌘7` | `NewKimiAgent` | `on_new_kimi_agent` | `/new kimi` |
| New ZCode Agent | 未绑定 | `NewZCodeAgent` | `on_new_zcode_agent` | `/new zcode` |
| New InfCode Agent | 未绑定 | `NewInfCodeAgent` | `on_new_infcode_agent` | `/new infcode` |
| New Pi Agent | `⌘8` | `NewPiAgent` | `on_new_pi_agent` | `/new pi` |
| New Hermes Agent | `⌘9` | `NewHermesAgent` | `on_new_hermes_agent` | `/new hermes` |

除固定的 `Cmd+Q` 外，原生菜单暂不显示快捷键提示：可配置按键的真值仍是 `ShortcutSettingsState`，避免菜单显示与用户 override 不同步。Launchpad 中 Canvas 专属菜单项按原生禁用状态显示；`New Window` 与 `Open Project…` 保持可用。

底部 Dock 按「工作空间｜布局｜指令输入｜Voice｜设置」排列，布局、Voice 和设置沿用原有交互。创建按钮统一收纳到指令输入：

- `/new` 显示五种 Agent；`/new codex` 等直接创建，不再选择位置。
- `/terminal` 创建并激活终端卡片。快捷指令列表不提供创建浏览器和查看文件。
- `/theme` 显示正式六主题；`/theme midnight-violet` 应用主题。
- 支持有限中文表达，例如「创建一个 Codex 卡片」「打开终端」「切换深色主题」。未知、否定、多操作或额外参数不会执行。
- 输入只过滤；↑/↓选择、Tab 补全、Enter/点击执行；Esc 收起并保留草稿。IME 组词期间不执行。
- 仅非空白输入显示建议，空输入聚焦不展开。建议抽屉从底部向上展开，清空或失焦后收回；连续输入可中途反向，减少动态效果时只淡入淡出。

成功清空输入；失败及文件选择取消保留草稿。输入框聚焦时文本编辑与方向键不会进入画布或 PTY。本版本不接入 Ego。

## 4. 键事件路由（非 Action）

在 `crates/app/src/app/keyboard.rs` 的 `on_key_down` / `on_key_up` / `on_modifiers_changed` 中处理。Settings 录制态优先级最高，其次是可配置 app 命令，然后才是以下手写输入行为。

### 4.1 语音输入

`VoiceInteraction` 是可配置 app 命令，默认 `⌘E`。用户可在 Settings → Keyboard Shortcuts 重新录制；底栏与 Voice Settings 文案会显示当前有效快捷键。

| 快捷键 | 阶段 | 行为 |
| --- | --- | --- |
| 配置的语音快捷键按下 | `on_key_down` | 按当前模式开始单次语音指令或持续聆听 |
| 配置快捷键的主键松开（单次 Hold） | `on_key_up` | 结束输入并提交最终转写 |
| 配置所需修饰键松开（单次 Hold） | `on_modifiers_changed` | 结束输入 |

实现：`ShortcutSettingsState::voice_interaction_keystroke` + `crates/app/src/app/voice/control.rs` 中的 hold 匹配辅助函数。

**冲突隔离：**

- `⌘V`（粘贴）仍为只读保留输入，不能分配给语音或其它命令。
- 其它已占用的可编辑命令快捷键不能重复分配。

### 4.2 粘贴（`⌘V`）

不经 GPUI Action，在 `on_key_down` 中读取剪贴板并发送 `NodeInput::Paste` 到 PTY；
普通键入与 IME commit 使用 `NodeInput::Text`，避免被 bracketed-paste 包装后延迟 shell 的交互刷新。

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
- 被 app shortcut registry 消费的键（如默认 `⌘1`～`⌘5`、`⌘D`、`⌘M`、`⌘Esc` 等）

**会传入 PTY：**

- 字母、数字、符号（含 IME  finalized 字符的 `key_char` 路径）
- `Enter`、`Tab`、`Backspace`、`Esc`、方向键
- `Shift+Enter`：所有 PTY 卡片共用同一策略；已请求 Kitty 增强键盘协议时发送
  `CSI 13;2u`；仅启用 bracketed paste 时发送包裹的 LF，支持 Claude 等 TUI 换行而不提交。
  两种协议均未启用时保留 legacy `CR`；普通 `Enter` 始终保持原有提交行为。
- `Option+Enter`：终端应用请求增强键盘协议时发送 `CSI 13;3u`，供 Codex 等 TUI 插入换行；
  未请求时回退为 legacy `CR`
- `Ctrl+*`、`Alt+*` 组合（如 `Ctrl+C` → ETX）

> 注：默认快捷键改为 `⌘Esc` 后，裸 `Esc` 不再被 registry 消费，会作为 `Esc` 键传入 PTY（终端进程可见）。`⌘Esc` 才触发 `ExitFocus`，把键盘控制权交还画布。

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
- Command Dock 已移除 Select/Scroll 模式按钮，目前没有 UI 入口可开启；开启后滚轮才会平移 `viewport.pan`。
- 当前实现**不会**通过滚轮改变 `viewport.zoom`（旧 zoom 行为已移除，见 `canvas_view_tests.rs`）。

## 6. 输入模式速查

| 模式 | 进入方式 | 退出方式 | 键盘主要行为 |
| --- | --- | --- | --- |
| `CanvasSelect` | 默认；单击选中；Esc | 双击/正文点击 → 聚焦 | 方向键导航；Space 预备 pan；平台快捷键走 shortcut registry |
| `NodeFocused` | 双击；terminal 正文点击；键盘自动聚焦 | `⌘Esc` → `ExitFocus` | 按键进 PTY（含裸 `Esc`）；`⌘V` 粘贴；`⌘C` 复制选区 |

路由决策的纯函数：`crates/runtime/src/route.rs`。

## 7. 预留 / 未实现

| 项目 | 状态 |
| --- | --- |
| 输入路由类快捷键编辑 | `⌘V`、方向键、`Space`、PTY 输入在 Settings 中只读展示；`VoiceInteraction` 可编辑 |
| 滚轮缩放 | 文档旧描述（`ARCHITECTURE.md` §5）已过时；当前滚轮不缩放 |

## 8. 源码索引

| 关注点 | 文件 |
| --- | --- |
| 快捷键 registry / 偏好 / 校验 | `crates/app/src/shortcuts.rs` |
| Settings 快捷键页面 | `crates/app/src/settings/keyboard.rs` |
| Action 定义与 `on_action` 挂载 | `crates/app/src/app/mod.rs` |
| 键事件 / app 命令分发 / 粘贴 / 导航 / 复制 / 删除 | `crates/app/src/app/keyboard.rs` |
| 语音长按快捷键 | `crates/app/src/app/voice/control.rs` |
| GPUI → PTY 编码 | `crates/app/src/cards/term/view.rs` → `keystroke_to_input` |
| IME  defer 判定 | `crates/app/src/app/ime.rs` |
| 画布 pan / 选中 / 文本选区 | `crates/app/src/canvas/view.rs` |
| 滚轮路由 | `crates/app/src/canvas/scroll.rs` |
| 纯路由策略 | `crates/runtime/src/route.rs` |
| 方向键空间导航 | `crates/canvas/src/navigation.rs` |

## 9. 新增快捷键检查清单

1. 在 `actions!` 中声明 Action（若需菜单复用）。
2. 在 `shortcuts.rs` 的 `default_shortcut_entries()` 增加命令默认值和 Settings 展示文案。
3. 在 `settings/keyboard.rs` 增加 `settings-shortcut-*` 与 restore id，保证 Settings → Keyboard Shortcuts 可录制与恢复。
4. 在 `app/keyboard.rs` 的 `dispatch_shortcut_command` 映射到处理器。
5. 在 `MultisoulApp::render` 根元素 `.on_action(...)` 挂载菜单 Action 处理器（仅当需要菜单复用）。
6. 若快捷键不应传入 PTY，确认 `keystroke_to_input` 对 `modifiers.platform` 的拦截已覆盖；必要时补测试（参考 `test_keystroke_to_input_platform_shortcut_is_not_sent_to_pty`）。
7. 若与手写输入路由冲突（如 `⌘V`），先在 `shortcuts.rs` 保留输入行为，再在 `keyboard.rs` 明确优先级并补单元测试。`VoiceInteraction` 走可编辑 registry，Hold 松键逻辑需同步读取当前配置。
8. 更新本文档。
