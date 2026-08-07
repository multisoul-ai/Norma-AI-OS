# AgentCard 输入延迟与卡顿修复实施计划

> 状态：Planned / 待实施  
> 日期：2026-08-06  
> 范围：AgentCard、Terminal 输入链路、Runtime drain、Canvas/Terminal 渲染  
> 原则：先解决可感知延迟，再以真实性能数据决定是否进入结构性重构

## 1. 核心结论

AgentCard 当前不是本地文本输入框，而是 Terminal-backed Live Node。普通输入必须经过：

```text
键盘或 IME
  → Runtime::send_node_input
  → 同步写入 PTY
  → Agent TUI 回显
  → Runtime 周期性 drain
  → RepaintGate 合并
  → Canvas 重绘
  → Terminal 全帧准备与绘制
```

当前卡顿由两类问题叠加：

1. 输入延迟：PTY 回显被 Runtime 消费后，聚焦 Agent 仍要额外等待一轮 16ms repaint budget。
2. 渲染负载：按键可能先触发一次旧快照重绘；真实回显到达后又触发整张终端快照、整张 Canvas 的重建。

推荐方案不是简单把 16ms 定时器缩短，而是：

- 聚焦 Agent 自身产生输出时，在首次 drain 后立即请求下一显示帧。
- 普通按键没有本地视觉变化时，不提前重绘旧 Canvas。
- 用精确 dirty NodeId 限定交互优先级，避免后台 Agent 恢复高频重绘。
- 以真实性能数据决定是否继续实施 PTY 事件唤醒、卡片级局部失效和 dirty-row 增量渲染。

## 2. 已确认的当前行为

### 2.1 AgentCard 复用 Terminal 渲染

- `crates/app/src/cards/agent/card.rs:44`：AgentCard body 调用 `render_terminal_node`。
- `crates/app/src/app/keyboard.rs:381`：聚焦节点接收键盘事件。
- `crates/runtime/src/nodes/term.rs:158`：`NodeInput` 被写入 `TerminalSession`。
- `crates/terminal/src/pty.rs:185`：PTY 写入使用同步 `write_all` 和 `flush`。

因此字符不能像本地输入框一样立即进入可见缓冲区，必须等待 Agent TUI 回显。

### 2.2 Runtime 与 repaint 存在两级等待

- `crates/app/src/app/frame_loop.rs:20`：Runtime 周期固定为 16ms。
- `crates/app/src/app/frame_loop.rs:222`：每个周期调用 `runtime.drain_all()`。
- `crates/app/src/app/frame_loop.rs:290`：dirty 后仅向 `RepaintGate` 提交请求。
- `crates/app/src/app/frame_schedule.rs:123`：pending age 达到预算后才允许 repaint。

现有 `crates/e2e/src/e2e_19_terminal_repaint_coalescing.rs` 固化了以下契约：

- 16ms 时 Runtime 已得到最终 grid。
- 首次 drain 只把 repaint 置为 pending，Canvas render 仍为 0。
- 32ms 时才允许 Canvas 绘制。

### 2.3 普通按键可能提前重绘旧内容

`crates/app/src/app/keyboard.rs:395-403` 在写入 PTY 后无条件调用 `cx.notify()`。此时 PTY 回显通常尚未被 drain，所以 Canvas 可能先绘制一次旧快照，再在真实输出到达后重新绘制。

### 2.4 单字符变化仍是全终端、全 Canvas 粒度

- `crates/runtime/src/nodes/term.rs:66`：dirty 后创建完整 `GridSnapshot`。
- `crates/terminal/src/grid.rs:224`：复制完整 `rows × columns` 网格。
- `crates/app/src/cards/term/render_cache.rs:95`：revision 变化导致 prepared frame cache miss。
- `crates/app/src/cards/term/paint/prepared.rs:27`：重建 terminal layout 并 shape text runs。
- `crates/app/src/canvas/view.rs:46`：Canvas render 遍历所有节点。
- `crates/app/src/app/canvas_surface.rs:17`：根 `MultisoulApp::notify` 会失效 CanvasSurface。

### 2.5 繁忙 Agent 会放大帧尖峰

`crates/terminal/src/session.rs:80` 中的 `TerminalSession::drain()` 会在一次调用中持续排空当前通道，没有单帧数据量或时间预算。Agent 同时流式输出时，ANSI 解析和快照更新可能超过单帧预算。

## 3. 修复目标与非目标

### 3.1 目标

- 消除聚焦 Agent 输出在首次 Runtime drain 后额外等待的 16ms。
- 普通输入没有本地视觉变化时，不重绘旧 Canvas。
- 不因优化聚焦输入而提高后台 Agent 的重绘频率。
- 保证字符、Tab、Paste、Ctrl+C 和 IME commit 不丢失、不乱序。
- 建立 input-to-visible 的 P50/P95 指标和可复现 L3 场景。
- 只有 profile 证明必要时，才进入卡片级 Entity 和 dirty-row 重构。

### 3.2 非目标

- 不把 AgentCard 改成脱离 PTY 的原生聊天输入框。
- 不对普通字符做本地乐观回显。
- 不更换 GPUI、alacritty_terminal 或 portable-pty 版本。
- 不通过关闭动画、工作蛇、光标或语音视觉效果换取性能。
- 不在第一阶段实施完整 Canvas/Terminal 渲染架构重写。

## 4. 分阶段实施计划

| 阶段 | 目标 | 预计投入 | 启动条件 |
|---|---|---:|---|
| P0 | 建立输入到绘制的延迟基线 | 0.5～1 天 | 必须 |
| P1 | 消除额外等待和无效按键重绘 | 1～2 天 | 必须 |
| P2 | PTY 输出事件唤醒与有界 drain | 2～4 天 | P1 后仍有轮询或流式输出卡顿 |
| P3 | Agent/Terminal 卡片级局部失效 | 7～12 天 | 全局 Canvas 重建仍是主线程热点 |
| P4 | dirty-row 增量终端渲染 | 10～20 天 | 单卡片全行准备仍是主要热点 |

## 5. P0：性能基线与可观测性

### 5.1 增加 opt-in 延迟诊断

扩展 `crates/app/src/frame_diagnostics.rs`，记录以下不含用户内容的时间点：

- `input_received`
- `pty_write_completed`
- `pty_output_received`
- `runtime_drained`
- `repaint_requested`
- `canvas_render_started`
- `terminal_frame_prepared`

诊断只记录 NodeId、revision、时间差和场景状态，不记录输入文字、终端输出或会话内容。

由于 Agent TUI 可能将一次输入转换为复杂 ANSI 输出，第一阶段把“输入后目标节点的第一个新 revision”定义为近似回显点，不尝试解析输出内容建立字符级关联。

### 5.2 基线场景

在同一台参考机分别记录：

1. 单个 Agent 空闲时连续英文输入。
2. 单个 Agent 流式输出时输入。
3. Canvas 上分别存在 1、5、10 张可见卡片。
4. 英文直接输入与中文 IME 输入。
5. 受控 `/bin/cat` 或 scripted PTY 与真实 Codex/Claude TUI 对比。

通过受控 PTY 和真实 Agent 的差异，区分 Norma 调度成本与外部 TUI 回显成本。

### 5.3 P0 交付物

- 每个场景的 input-to-drain、drain-to-notify、notify-to-render、input-to-visible P50/P95。
- 同期 Canvas render 次数、terminal prepare 次数、主线程帧耗时和 CPU。
- P1 前可重复执行的基线报告。

## 6. P1：交互优先级与无效重绘修复

### 6.1 Runtime 返回精确 dirty NodeId

修改：

- `crates/runtime/src/runtime.rs`
- `crates/runtime/src/live.rs`（若需要统一报告契约）
- `crates/runtime/tests/unit/runtime_tests.rs`
- `crates/app/src/app/frame_loop.rs`

建议契约：

```rust
pub struct RuntimeDrainReport {
    pub dirty_nodes: Vec<NodeId>,
    pub runtime_changed: bool,
}
```

约束：

- 一个节点在单轮内收到多个 chunk，只能出现一次。
- NodeId 顺序保持 Document/Runtime 的确定性顺序。
- 不增加第二次全节点扫描。
- 保留现有 status → runtime event 行为。

### 6.2 聚焦 Agent 在首次 drain 后立即 repaint

修改：

- `crates/app/src/app/frame_loop.rs`
- `crates/app/src/app/frame_schedule.rs`
- `crates/app/tests/unit/frame_schedule_tests.rs`

本轮先聚合所有 repaint reasons，再统一决策，避免同一 tick 因多个 reason 重复 notify。

策略：

```text
dirty_nodes 包含当前聚焦且运行中的 Agent/Terminal
  → 本轮 drain 后立即 cx.notify()

仅后台节点 dirty
  → 继续使用 33ms 合并预算

Browser、Agent Hook、非交互状态变化
  → 保持原有调度策略
```

不能仅根据“当前存在一个聚焦 Terminal”给所有 Runtime dirty 交互优先级，否则后台 Agent 输出会绕过节流。

### 6.3 普通输入只在本地视觉状态变化时 notify

修改：

- `crates/app/src/app/keyboard.rs`
- `crates/app/src/app/keyboard/agent_interrupt.rs`
- `crates/app/src/app/keyboard/tab.rs`
- `crates/app/src/app/ime.rs`
- `crates/app/src/canvas/interaction.rs`

建议把相关函数调整为返回是否改变了本地视觉状态：

- `clear_text_selection_for_node(...) -> bool`
- `end_active_agent_task_for_manual_interrupt(...) -> bool`
- `interrupt_initialization(...) -> bool` 已有该契约。

调用端聚合：

```rust
let local_visual_changed =
    selection_cleared || initialization_interrupted || agent_task_state_changed;

if local_visual_changed {
    cx.notify();
}
```

普通字符、Tab、Paste、IME commit 只写入 PTY；真实回显由 Runtime dirty 路径触发绘制。Ctrl+C 改变任务状态、清除选区或中断初始化动画时仍然立即通知。

### 6.4 P1 验收结果

- 首次 Runtime drain 已得到新 grid 时，同一轮必须触发 Canvas repaint。
- 不再等待第二个 16ms tick。
- 无选区、无初始化动画、无任务状态变化的普通输入，不得提前重绘旧 Canvas。
- 后台 Agent 输出仍保留合并策略。
- 空闲聚焦 Agent 输入 P95 目标 `< 40ms`，且相对基线至少改善 12～16ms。

## 7. P2：事件驱动唤醒与有界 drain

P1 达标后不默认实施 P2。只有以下任一现象仍存在时启动：

- input-to-drain 仍稳定消耗接近 16ms。
- Agent 流式输出时 Runtime drain 明显超过主线程预算。
- 多 Agent 场景存在输出饥饿或长尾帧。

### 7.1 PTY 输出事件唤醒

修改候选：

- `crates/terminal/src/pty.rs`
- `crates/terminal/src/session.rs`
- `crates/runtime/src/nodes/term.rs`
- `crates/app/src/app/frame_loop.rs`

设计：

- 在 terminal 层定义 GPUI 无关、非阻塞的 `OutputWakeSink`。
- PTY reader 成功发送 Output 后，再向容量为 1 的 wake channel 发信号。
- App Runtime loop 使用 `select(output_wake, fallback_timer)`。
- PTY reader 不得直接调用 GPUI 或更新 Runtime。
- 容量为 1，连续输出合并为一个 pending wake，避免 wake storm。
- 16ms timer 保留为丢失唤醒和非终端任务的兜底。

### 7.2 有界 Terminal drain

建议报告：

```rust
pub struct TerminalDrainReport {
    pub dirty: bool,
    pub has_more: bool,
    pub bytes_processed: usize,
}
```

约束：

- 达到单轮时间或数据预算后停止，并安排立即续跑。
- 不丢弃任何 PTY 字节。
- 保持 ANSI 解析和 terminal replies 的顺序。
- 多节点采用轮转公平处理，单个高速 Agent 不能长期占满本轮预算。
- `Exited` 状态不能因输出积压长期延迟。
- 预算必须由 P0 benchmark 吞吐和 16.67ms 显示帧预算推导，代码注释写明算式，不直接使用无来源魔数。

## 8. P3：Agent/Terminal 卡片级局部失效

本阶段复用并落实 `docs/exec-plans/2026-07-25-cpu-p2-p3-local-invalidation-and-incremental-rendering.md` 中的 P2 方向。

目标结构：

```text
CanvasSurfaceView
  ├─ NodeCardView<Agent A>
  ├─ NodeCardView<Agent B>
  ├─ NodeCardView<Preview>
  └─ NodeCardView<Browser>
```

实施步骤：

1. 复用 P1 的精确 dirty NodeId。
2. 建立稳定的 `NodeCardStore<NodeId, Entity<NodeCardView>>`。
3. 先迁移 Terminal 和 Agent，形成 tracer bullet。
4. A 输出时只更新 A 的 presentation 并通知 A Entity。
5. Canvas 只在节点增删、viewport、布局、选择或其他结构状态变化时重建。
6. Terminal/Agent 验证稳定后，再迁移 Preview、Browser 和 Rect。

GPUI 约束：

- Parent 持有强 `Entity`，Card 回传使用 `WeakEntity`，避免引用环。
- 所有 observer/subscription 必须保存在字段中。
- 先计算不可变 presentation，再更新子 Entity，避免父子 Entity 嵌套 update。
- 节点删除必须同时释放 Entity、subscription、terminal cache 和 native browser view。
- Browser native view 生命周期与几何权威继续留在 Canvas/App 层。

## 9. P4：dirty-row 增量 Terminal 渲染

只有 profile 证明单张 AgentCard 的完整 snapshot、layout 或 `shape_line` 仍是主要热点时启动。

建议域契约：

```rust
pub enum TerminalFrameUpdate {
    Full(FullSnapshot),
    Partial {
        base_revision: u64,
        revision: u64,
        damaged_rows: Vec<RowUpdate>,
    },
}
```

处理原则：

- 普通输入通常只更新光标所在行及必要的相邻行。
- prepared terminal frame 按行结构共享和缓存。
- resize、scroll、theme、zoom、selection、alternate screen、revision 不匹配时安全回退 Full。
- 完整 `snapshot()` 保留为正确性 oracle。
- 增量应用结果必须与完整 snapshot 逐 Cell 一致。

## 10. 中文 IME 的独立体验优化

当前 `replace_and_mark_text_in_range` 只保存 marked range，没有在 AgentCard 内绘制预编辑文本。中文用户可能感到组合文字已经输入，但卡片内部没有反馈。

该问题与普通字符回显延迟相关但不等价，建议单独实施：

- 在 terminal cursor 位置绘制轻量 marked-text overlay。
- 只显示尚未 commit 的组合文本，不提前写入 PTY。
- commit 后立即清除 overlay，由 TUI 回显接管正式内容。
- 候选窗口定位继续使用现有 terminal cursor bounds。
- 遵循 Apple/macOS marked text 的下划线、选中范围和辅助功能语义。

这属于用户可见 UI 改动，实施前必须加载 `apple-design`，验收时必须使用 Computer Use 操作真实 App。

## 11. 不采用的方案

### 11.1 不直接把 Runtime 周期改成 8ms

这会增加 CPU 和唤醒次数，但不会解决：

- dirty 后额外等待 RepaintGate。
- 输入前的旧快照重绘。
- 整张 Canvas 和整张 terminal frame 重建。

### 11.2 不对普通输入做本地乐观回显

Agent TUI 可能处于 raw mode、补全、审批、全屏刷新或自定义键盘协议状态。本地提前插入字符会造成重复字符、光标错位或与 TUI 权威状态冲突。

### 11.3 不彻底关闭 RepaintGate

后台多个 Agent 流式输出时需要继续合并刷新。交互优先级必须绑定“聚焦节点自身 dirty”，而不是全局关闭节流。

### 11.4 不直接从 P4 开始

dirty-row 工程量最大，而 P1 可以用较小改动消除确定存在的一轮 16ms 等待。必须先获得 P1 后 profile，再决定结构重构是否值得。

## 12. 自动化测试计划

所有新增测试必须遵守仓库的完整测试注释、数值推导、正负断言和断言说明规则。

### 12.1 Runtime 单测

- A dirty 时报告精确包含 A。
- B 未变化时不得错误包含 B。
- A 收到多个 chunk 时只出现一次。
- A/B 同时变化时各出现一次，顺序确定。
- 无 dirty 时集合为空。
- terminal status/runtime event 行为保持不变。

### 12.2 FrameSchedule 单测

- 聚焦且运行中的 Agent 自身 dirty，在第一次 drain tick 即允许 repaint。
- 非聚焦节点 dirty 不得继承交互优先级。
- 后台输出继续使用 33ms 合并。
- 同一 tick 的多个 repaint reason 只产生一次 emission。
- Settings、后台窗口和 Voice 优先级行为不回归。

### 12.3 输入路由单测

- 普通字符没有本地视觉变化时不调用旧快照 notify。
- 清除选区时必须 notify。
- 中断初始化动画时必须 notify。
- Ctrl+C 改变 Agent task state 时必须 notify。
- Tab、Paste 和 IME commit 保持原始字节及顺序。

### 12.4 E2E

更新 `crates/e2e/src/e2e_19_terminal_repaint_coalescing.rs`：

- 15ms 时 grid 不得提前出现最终输出。
- 16ms 首次 drain 后 grid 必须更新且 Canvas 至少 render 一次。
- 不再要求等到 32ms 才 render。
- burst 必须保留最终帧，不得残留旧帧。

新增用例：

- 普通字符写入、PTY 尚未回显时，不得无意义重绘 Canvas。
- PTY 回显后必须绘制最终内容。
- 两个 Agent 中 A 输入不得错误更新 B 的 NodeCard Entity。
- Ctrl+C、Tab、Paste、中文 IME commit 不丢失、不乱序。
- 高速输出达到 drain budget 后，后续输出仍能完整消费。

### 12.5 L3 真实 App

- 真实 `.app` 中操作 Codex/Claude AgentCard。
- 空闲 Agent 连续输入。
- Agent 流式输出期间输入。
- 多卡片 Canvas。
- 中文 IME 组合与 commit。
- Ctrl+C、Tab、Paste 和 scrollback 回归。

涉及 NodeCard Entity 或 IME overlay 的视觉变化时，必须使用 Computer Use 完成真实 App 验收；headless GPUI 和静态截图只能作为补充。

## 13. 性能验收标准

### P1 门禁

- 受控 PTY 的首次 drain 后，同一轮产生 repaint。
- drain-to-notify P95 不再固定包含额外 16ms。
- 空闲聚焦 Agent input-to-visible P95 `< 40ms`。
- 相对基线至少改善 12～16ms。
- 普通按键在回显前不产生旧 Canvas render。
- 后台 Agent repaint 频率不得显著上升。

### P2 门禁

- output wake 后无需等待固定 16ms timer。
- 流式输出期间 input-to-visible P95 `< 50ms`。
- 单个高速 Agent 不得让其他 Agent 输出长期饥饿。
- drain 不丢输出，最终 grid 与无预算版本一致。

### P3 门禁

- A dirty 时 A NodeCard render，B/Preview/Browser 不得因 A 的 terminal revision 变化而 render。
- 两个持续更新的 Agent 中位 CPU 目标 `< 20%`。
- 静止 Canvas 中位 CPU目标 `< 5%`。
- 焦点、选择、拖拽、resize、关闭和 Browser native view 无回归。

### P4 门禁

- 单行输入只重建 damage 指定行，不随总行数线性重建所有 prepared rows。
- Partial 结果与 Full snapshot 完全一致。
- resize、scroll 和 alternate screen 可安全回退 Full。

## 14. 验证命令

按阶段执行最小充分验证，最终合并前执行跨 crate 验证：

```bash
cargo test -p runtime
cargo test -p terminal
cargo check -p app
cargo test -p e2e
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
cargo test --workspace
```

真实 App 性能验收复用或扩展现有 CPU L3 runner，并新增 input-to-visible 延迟报告。测试输出必须明确区分 headless GPUI、真实 PTY 和真实 `.app`。

## 15. 风险与控制

| 风险 | 影响 | 控制措施 |
|---|---|---|
| 交互 repaint 放宽导致后台 CPU 回升 | 多 Agent 输出功耗上升 | 优先级只授予精确 dirty 的聚焦节点 |
| 移除按键 notify 后漏掉本地状态变化 | 选区或任务徽标不及时更新 | 所有本地状态修改函数返回 bool，并逐项测试 |
| PTY wake storm | 线程和主循环频繁唤醒 | 容量 1 非阻塞 wake channel，Runtime 批量 drain |
| drain budget 导致输出延迟或饥饿 | TUI 更新不完整 | 保序、不丢数据、轮转公平、立即安排续跑 |
| NodeCard Entity 生命周期错误 | 泄漏、悬空订阅、Browser 残留 | 强子 Entity、弱父引用、保存订阅、删除时统一清理 |
| 增量 terminal damage 错误 | 屏幕残影或 Cell 状态错误 | Full snapshot oracle，revision 不匹配时回退 Full |
| 真实 Agent TUI 自身回显缓慢 | Norma 优化后仍有长尾 | 受控 PTY 与真实 Agent 分开测量和报告 |

## 16. 推荐提交切片

1. `perf(runtime): report exact dirty node ids`
2. `perf(app): repaint focused terminal on first dirty drain`
3. `perf(app): avoid stale canvas repaint for ordinary terminal input`
4. `test(e2e): enforce first-drain terminal repaint latency`
5. 条件提交：`perf(terminal): wake runtime on pty output`
6. 条件提交：`perf(runtime): bound and fairly resume terminal drain`
7. 条件提交：`perf(app): isolate terminal and agent card invalidation`
8. 条件提交：`perf(terminal): apply dirty-row frame updates`

每个切片独立完成 Red → Green → Refactor 和对应验证，不把 P1～P4 合并成一次大改。

## 17. 决策建议

先实施 P0 与 P1。P1 是当前收益最高、风险最低的修复：它直接消除确定存在的一轮 16ms 等待，并减少普通按键触发的旧快照重绘。

P1 完成后重新采样：

- 已达到输入 P95 和 CPU 目标：停止，不进入结构性重构。
- input-to-drain 仍高：进入 P2 事件唤醒。
- Canvas render 仍是主线程热点：进入 P3 卡片级局部失效。
- 单卡 terminal prepare 仍随总行数增长：进入 P4 dirty-row。

该顺序以真实性能证据控制工程投入，同时保持 AgentCard 继续服从现有 Live Node、Terminal 和 Canvas 架构。
