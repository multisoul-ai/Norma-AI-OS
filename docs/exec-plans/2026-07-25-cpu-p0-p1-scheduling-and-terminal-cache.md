# Norma OS CPU 优化 P0/P1 实施计划

> 状态：Proposed  
> 日期：2026-07-25  
> 预计投入：6～9 人日  
> 实施方式：严格按纵向切片执行 TDD；每个切片先 Red，再最小 Green，完成后才进入下一个切片。

## 1. 目标

在不改变 Terminal、Agent 卡片及现有动画语义的前提下，完成此前定义的 P0/P1：

- P0：建立可重复的性能基线；空闲和后台时降低调度频率；把高频 Runtime dirty 合并到受控的前台刷新预算。
- P1：让重复读取的终端快照共享内存并带稳定 revision；缓存终端布局与文字 shaping，动画变化不得重新准备未变化的终端文本。

本计划解决：

```text
高频 PTY/动画变化
  × 根 Entity 整窗 notify
  × 每次重建所有可见终端 layout / shaped text
  = 持续高 CPU
```

## 2. 范围边界

### 本计划包含

- `app` 的 Runtime/animation frame 调度策略与 repaint 合并。
- `runtime` 的不可变共享 Terminal frame/revision 契约。
- `app` 层 GPUI 专用终端渲染缓存。
- L1/L2 功能回归和 L3 真实 `.app` 性能验收。
- 只增加 opt-in 性能诊断，不增加用户设置项。

### 本计划不包含

- 每张 Canvas 卡片拆成独立 GPUI Entity。
- 节点级局部失效。
- alacritty dirty-row 增量快照或逐行增量绘制。
- 升级 `gpui = 0.2.2`、`gpui-component = 0.5.1` 或 `alacritty_terminal = 0.26`。
- 以关闭工作蛇、呼吸光、光标或语音内缘光换取性能。

以上内容分别属于 P2/P3，见
[`2026-07-25-cpu-p2-p3-local-invalidation-and-incremental-rendering.md`](2026-07-25-cpu-p2-p3-local-invalidation-and-incremental-rendering.md)。

## 3. 已确认的工程事实

- Runtime frame 固定每 12ms 执行 `Runtime::drain_all()`；任一 Live Node dirty 后根 Entity 调用 `cx.notify()`。
- animation frame 默认也每 12ms 醒来；工作蛇、呼吸与运行中光标内部约按 33ms 放行 repaint。
- `MultisoulApp::render()` 会重新进入 `render_canvas()`，遍历所有可见节点并构造卡片 body。
- `TermLive` 已缓存 `NodeView`，但 `NodeView::Term` 当前深拷贝完整 `GridSnapshot`。
- terminal paint 每次从 snapshot 重建 runs，并在 paint 中逐 run 调用 `shape_line()`。
- GPUI 0.2.2 的 `ShapedLine` 可克隆，内部 layout 使用 `Arc`，适合留在 `app` 层缓存。

## 4. 实施前确认的测试 seam

TDD 开工前由实施者与用户确认以下 seam；未确认前不得写测试：

1. **调度策略 seam**：纯 Rust `FrameSchedule`/`RepaintGate`，输入时间和活动状态，输出下一次 tick 间隔及本 tick 是否允许 repaint。
2. **Runtime frame seam**：公开 `Runtime::node_view(NodeId)` 返回共享 snapshot 与单调 revision。
3. **终端准备缓存 seam**：`TerminalRenderCache` 输入 NodeId、render key 与共享 snapshot，输出 prepared frame 和 `Hit/Miss`。
4. **用户流程 seam**：`crates/e2e` 的公开 action + Test PTY，验证合并刷新后最终终端内容、焦点与输入仍正确。
5. **真实性能 seam**：打包后的真实 `.app`，固定工作区、固定节点数、固定采样时长，记录 Norma OS 与 WindowServer CPU。

测试不直接断言某个私有函数被调用多少次；只有 repaint/prepare 数量本身属于明确性能契约时，才通过公开诊断快照断言。

## 5. 验收标准与证据

| 验收标准 | 自动化证据 | 真实环境证据 |
|---|---|---|
| 空闲 Canvas 不再约 83Hz 唤醒 animation update | `frame_schedule` 单测 | L3 日志 tick 频率 |
| 单次 dirty 不会丢失，多个 dirty 在预算内只 repaint 一次 | `RepaintGate` 单测 + L1 burst e2e | frame diagnostics |
| Voice 内缘光继续保持 12～16ms、最高 60fps | 调度优先级单测 + 既有 Voice e2e | Computer Use 播放验收 |
| 相同 Terminal frame 重复读取不深拷贝 | Runtime `Arc::ptr_eq` + revision 单测 | allocation/sample 对比 |
| animation-only repaint 不重新 layout/shape 终端文本 | cache hit/miss 单测 + headless GPUI 测试 | `shape_line` 热点采样下降 |
| PTY 输出、resize、scroll、theme、zoom、selection 正确失效缓存 | Runtime/app 单测矩阵 | Computer Use 真实 App |
| Terminal/Agent 输入回显与最终 TUI 内容不丢失 | L1 + L2 e2e | 真实 Codex 会话 |
| 当前双 Codex 场景显著降 CPU | 非 CI 的 L3 runner | 相对基线下降 ≥50%；参考机目标 ≤20% |

建议参考机硬目标：

- 静止 Canvas：Norma OS 中位 CPU `< 5%`。
- 后台或最小化窗口：中位 CPU `< 2%`。
- 两个持续更新的 Codex Agent：中位 CPU `< 20%`。
- 聚焦终端输入到可见回显：P95 `< 50ms`。

若绝对 CPU 受机器环境干扰，发布门禁仍要求同一机器、同一场景、同一采样脚本下中位 CPU 相对基线下降至少 50%，并且不能牺牲功能或输入延迟。

## 6. 全局 TDD 规则

- 一个测试 → 观察 Red → 最小实现 → 观察 Green；禁止先批量写完整测试矩阵。
- 测试通过公共行为 seam，不 mock 自有内部模块。
- 时间使用显式 `Instant`/`Duration` 或 GPUI mock clock，不使用 wall-clock sleep 做单元断言。
- 每个测试必须写“场景、数据构造与数值推导、执行过程、预期结果”。
- 每个 `assert!`、`assert_eq!`、`assert_ne!` 必须有失败说明；关键状态逐项做正、负断言。
- 每个纵向切片开发中只跑聚焦测试；切片 Green 后再跑受影响 crate。
- 任何切片 15 分钟内不能得到聚焦 Green，停止扩散调查并报告具体阻塞。

## 7. 任务分解

### Task 0：冻结真实性能基线

**预计：0.5 人日**

**新增：**

- `scripts/run-cpu-performance-l3.sh`
- `scripts/cpu-performance-gate.py`
- `scripts/tests/test_cpu_performance_gate.py`
- `docs/references/cpu-performance-scenarios.md`
- `crates/app/src/app/cpu_benchmark.rs`
- `crates/app/tests/unit/cpu_benchmark_tests.rs`

**场景：**

1. 7 张可见卡片、无活动任务，采样 60 秒。
2. 1 个活跃 Codex Agent，采样 60 秒。
3. 2 个活跃 Codex Agent，采样 60 秒。
4. 同一窗口切到后台，采样 60 秒。

runner 必须记录：

- App 版本、commit SHA、macOS、CPU 核数、内存。
- 节点数、活跃 Agent 数、采样开始/结束时间。
- Norma OS CPU 的 median/P95/max。
- WindowServer CPU 的 median/P95/max。
- opt-in frame diagnostics 汇总。
- 原始采样数据和 `/usr/bin/sample` 输出路径。

结果写入 `artifacts/performance/cpu-p0-p1/<timestamp>/`，不进入 Git。

**完成条件：**

- 同一场景连续运行两次，median 差异不超过 20%；否则先修正场景稳定性，不进入优化。
- 保存本次已知的 28%～44% 活跃 CPU 作为历史参考，但正式 gate 只使用 runner 新生成的同机基线。

---

### Task 1：空闲/后台 animation 调度

**预计：0.5～1 人日**

**新增/修改：**

- 新增 `crates/app/src/app/frame_schedule.rs`
- 新增 `crates/app/tests/unit/frame_schedule_tests.rs`
- 修改 `crates/app/src/app/mod.rs` 挂载测试模块
- 修改 `crates/app/src/app/frame_loop.rs`

#### Cycle 1.1：空闲 Canvas 降频

**Red：**

先写 `test_animation_schedule_uses_250ms_when_canvas_and_voice_are_idle`。

数据必须明确：

```text
voice active = false
initialization/motion/cursor active = false
window active = true
expected interval = 250ms
negative control = 12ms
```

Run:

```bash
cargo test -p app test_animation_schedule_uses_250ms_when_canvas_and_voice_are_idle
```

Expected Red：尚无纯调度策略，或仍返回 12ms。

**Green：**

实现纯 `AnimationSchedule::next_interval(...)`，空闲前台返回 250ms。生产 loop 只消费决策，不在策略函数内访问 GPUI。

#### Cycle 1.2：活动 Canvas 保持平滑

**Red：**

写 `test_animation_schedule_keeps_active_canvas_at_16ms_or_next_visual_boundary`：

- 活动工作蛇/光标时基础预算为 16ms。
- 初始化动画若下一视觉边界小于 16ms，返回更小的正 Duration。
- 不得返回空闲 250ms。

**Green：**

让 `MotionStore` 暴露纯活动摘要，而不是让调度器解析内部节点状态。

#### Cycle 1.3：Voice 优先级

**Red：**

写 `test_animation_schedule_keeps_voice_inset_at_12ms_while_settings_or_background_occludes_canvas`：

- Voice PCM 正在播放时为 12ms。
- Settings 打开或窗口不活跃不能把 Voice 降到 250ms。
- Canvas-only 动画在同样遮挡状态下必须降频/暂停。

**Green：**

Voice 与 Canvas 使用不同 activity bit；调度优先级为 Voice > 可见 Canvas 动画 > idle。

**切片验证：**

```bash
cargo test -p app frame_schedule
cargo test -p app frame_loop
cargo test -p app cursor_animation
```

---

### Task 2：Runtime repaint 合并，不丢最后一帧

**预计：1～2 人日**

**修改：**

- `crates/app/src/app/frame_schedule.rs`
- `crates/app/src/app/frame_loop.rs`
- `crates/app/tests/unit/frame_schedule_tests.rs`
- `crates/app/tests/unit/frame_loop_tests.rs`

#### Cycle 2.1：预算内多个 dirty 合并

**Red：**

写 `test_repaint_gate_coalesces_three_runtime_changes_into_one_16ms_repaint`。

数值推导：

```text
focused budget = 16ms
dirty timestamps = 0ms, 4ms, 8ms
8ms < 16ms → 前三次都只保持 pending
tick at 16ms → 恰好放行 1 次
```

正断言：16ms 时放行；负断言：4ms/8ms 不放行，累计放行数不为 3。

**Green：**

实现纯 `RepaintGate`：

- `request(reason)` 只置 pending 并合并 reason。
- `poll(now, budget)` 到期后只放行一次并清 pending。
- `reset()` 用于 Settings 遮挡或 screen 离开 Canvas。

#### Cycle 2.2：单次 dirty 最终必须绘制

**Red：**

写 `test_repaint_gate_preserves_single_pending_change_until_deadline`：

- 0ms 仅一次 dirty。
- 之后没有新 dirty。
- 15ms 不放行；16ms 必须放行。
- 32ms 不得重复放行。

这个测试防止“只在新 dirty 到达时检查预算”导致最终 PTY 输出永远不显示。

**Green：**

Runtime loop 每次 tick 都 poll pending gate；不能只在 `runtime_changed=true` 的分支 poll。

#### Cycle 2.3：前台与后台预算

**Red：**

写两个独立测试：

- `test_repaint_budget_is_16ms_for_focused_running_terminal`
- `test_repaint_budget_is_33ms_for_background_terminal_output`

不得把用户键盘输入走 100ms 级全局节流。后台窗口且无 Voice 时允许暂停 Canvas repaint，但 Runtime 仍必须 drain，防止 PTY channel 无界积压。

**Green：**

用纯 `RepaintContext` 表达 screen、window active、focus 和 Voice 状态；frame loop 不散落条件分支。把前台 Runtime drain 基础间隔从 12ms 收敛到 16ms，使 focused dirty 的最坏等待仍落在单个 60Hz 显示帧附近；background 33ms 是最小 repaint 间距，实际放行可落在下一个 Runtime tick，但输入回显 P95 必须继续满足 `< 50ms`。

同步修改 e2e harness：不得继续把 12ms 写死在 `drain_until`。测试时钟步长必须读取与生产一致的 test-support timing contract，后续调整调度策略时由同一契约驱动。

**切片验证：**

```bash
cargo test -p app repaint_gate
cargo test -p app frame_loop
cargo check -p app
```

---

### Task 3：调度诊断和 L1 “不丢最终输出”闭环

**预计：0.5～1 人日**

**修改：**

- `crates/app/src/frame_diagnostics.rs`
- `crates/app/tests/unit/frame_diagnostics_tests.rs`
- `crates/e2e/src/harness.rs`
- 新增 `crates/e2e/src/e2e_19_terminal_repaint_coalescing.rs`
- `crates/e2e/src/lib.rs`

#### Cycle 3.1：诊断计数

**Red：**

写 `test_frame_diagnostics_distinguishes_requested_coalesced_and_emitted_repaints`。

给定 3 次 request、2 次被合并、1 次 emitted，报告必须逐项为 `3/2/1`，且不能把 coalesced 误报成 dropped。

**Green：**

仅在 `MULTISOUL_FRAME_DIAGNOSTICS` 开启时累计/输出：

- requested repaint reasons
- coalesced requests
- emitted repaint
- pending age
- selected budget

生产默认关闭，不在 hot path 拼接字符串。

#### Cycle 3.2：L1 burst 最终帧

**Red：**

写 `e2e_19_terminal_burst_is_coalesced_without_losing_final_grid_state`：

- Test PTY 在 8ms 内发送三帧 `frame-1`、`frame-2`、`frame-3`。
- mock clock 先推进到预算前，再推进到预算边界。
- 预算前不得报告 emitted repaint。
- 边界后 grid 必须包含 `frame-3`，不得停留在 `frame-1` 或 `frame-2`。
- emitted 必须少于 requested。

**Green：**

只增加最窄的 `#[cfg(any(test, feature = "test-support"))]` 诊断投影；不得为 e2e 暴露生产可变内部状态。

**切片验证：**

```bash
cargo test -p app frame_diagnostics
cargo test -p e2e e2e_19_terminal_burst
```

---

### Task 4：共享 Terminal snapshot 与 revision

**预计：1～2 人日**

**修改：**

- `crates/runtime/src/live.rs`
- `crates/runtime/src/nodes/term.rs`
- `crates/runtime/tests/unit/runtime_tests.rs`
- 所有 `NodeView::Term` 消费点
- `crates/e2e/src/harness.rs`

目标契约：

```rust
NodeView::Term {
    snapshot: Arc<GridSnapshot>,
    revision: u64,
    status: TerminalStatus,
}
```

revision 只表达该 NodeView 的可渲染终端帧变化，单调递增；不使用全局 revision。

#### Cycle 4.1：不变 frame 共享身份

**Red：**

写 `test_term_node_view_reuses_arc_and_revision_until_terminal_changes`：

- 初始 revision 记为 `r0`。
- 连续读取三次 `node_view`。
- 三次 `Arc::ptr_eq` 为 true，revision 均为 `r0`。
- refresh count 不得从 1 变为 3。

**Green：**

`TermLive` 存 `Arc<GridSnapshot>` 和 revision；`view()` 只克隆 Arc。

#### Cycle 4.2：PTY 输出生成新 frame

**Red：**

写 `test_term_node_view_advances_revision_once_for_drained_output`：

- echo/scripted PTY 注入 `"hi"`。
- drain 前读取 `r0`；一次 `drain_all()` 后读取 `r1`。
- `r1 > r0`，新旧 Arc 不同，frame 含 `"hi"`。
- 再次无输出 drain 后 revision 不得继续增加。

**Green：**

只在 `session.drain()` 返回 dirty 时构造新 Arc 并增加 revision。

#### Cycle 4.3：resize/scroll/status 失效

依次做三个独立 Red/Green，不批量预写：

- `test_term_resize_advances_revision_and_updates_dimensions`
- `test_term_scrollback_change_advances_revision_and_updates_offset`
- `test_term_exit_status_advances_revision_without_losing_last_frame`

**兼容修改：**

- app 渲染接受 `Arc<GridSnapshot>`，避免转回深拷贝。
- e2e `read_grid` 若需要 owned value，明确在测试边界克隆；生产绘制不得克隆完整 rows。

**切片验证：**

```bash
cargo test -p runtime term_node_view
cargo test -p runtime
cargo test -p e2e e2e_02_terminal
cargo check -p app
```

---

### Task 5：缓存 Terminal layout 与 shaped text

**预计：3～5 人日**

**新增/修改：**

- 新增 `crates/app/src/cards/term/render_cache.rs`
- 新增 `crates/app/tests/unit/cards/term/render_cache_tests.rs`
- 修改 `crates/app/src/cards/term/mod.rs`
- 修改 `crates/app/src/cards/term/paint.rs`
- 修改 `crates/app/src/cards/term/view/render.rs`
- 修改 `crates/app/src/cards/mod.rs`
- 修改 `crates/app/src/app/mod.rs` / `bootstrap.rs`
- 修改 Canvas 节点删除或 cache retain 路径

**架构决定：**

- 缓存必须位于 `app`；`terminal`/`runtime` 不得出现 GPUI 类型。
- `TerminalRenderCache` 每个 NodeId 最多保留当前 prepared frame，不做无界历史缓存。
- key 使用值比较，不要求 Theme 实现 Hash，至少包含：
  - NodeId
  - terminal revision
  - terminal metrics（font size/line height/cell width）
  - 终端相关 theme/palette/glass tokens
  - selection
  - viewport/scroll display offset 已由 revision 覆盖
- cursor phase、工作蛇 phase、普通 Canvas selection chrome 不进入 terminal text key。
- prepared frame 缓存 backgrounds、selection geometry 和可克隆 `ShapedLine`；paint 只计算位置并提交 primitive。
- GPUI shaping 只在前台 prepaint 执行；不得把 `Window::text_system()` 移到后台线程。
- 推荐用 App 持有、UI 线程独占的 cache handle，让 `canvas` prepaint 闭包取得 prepared frame；不得在 domain crate 引入 `RefCell` 或 GPUI。

#### Cycle 5.1：同 key 命中

**Red：**

写 `test_terminal_render_cache_reuses_prepared_frame_for_same_revision_and_inputs`：

- 同 NodeId、revision=7、同 metrics/theme/selection 连续 resolve 两次。
- 第一次 `Miss`，第二次 `Hit`。
- prepared frame 身份相同。
- prepare count 为 1，不得为 2。

**Green：**

只实现单 entry-per-node 的 key/value cache，不提前实现失效矩阵。

#### Cycle 5.2：cursor/工作蛇不使终端缓存失效

**Red：**

写 `test_terminal_render_cache_ignores_cursor_and_snake_phase`：

- 相同 terminal key。
- cursor phase 从 0.0 变 0.5；snake phase 从 0.1 变 0.8。
- terminal prepared frame 仍为 Hit。
- cursor paint model 和 snake visual 必须发生变化，证明不是把动画冻结。

**Green：**

把 cursor 保持为独立 paint layer；工作蛇继续位于 card body 外侧 overlay。两者只消费 prepared terminal frame，不参与 key。

#### Cycle 5.3：内容与几何正确失效

逐个 Red/Green：

- `test_terminal_render_cache_misses_when_revision_changes`
- `test_terminal_render_cache_misses_when_metrics_change_after_zoom`
- `test_terminal_render_cache_misses_when_terminal_theme_tokens_change`
- `test_terminal_render_cache_misses_when_selection_changes`

每个测试都要同时包含一个不应失效的负对照，避免“永远 Miss”也能通过。

#### Cycle 5.4：shaping 真正被复用

**Red：**

使用 GPUI headless Window 写
`test_terminal_prepared_frame_shapes_text_once_across_animation_only_repaints`：

- 两行固定文本，各自产生已知数量 run。
- 连续三次 animation-only repaint。
- shaped run 内容和位置每次可绘制。
- shape prepare count 只等于首次 run 数，不得乘以 3。

**Green：**

把当前 `paint_text_run()` 内的 `shape_line()` 前移到 cache miss 的 prepare 阶段；paint 阶段复用 `ShapedLine::paint()`。

#### Cycle 5.5：节点删除释放缓存

**Red：**

写 `test_terminal_render_cache_removes_closed_node_without_evicting_live_node`：

- cache 含节点 A/B。
- Runtime 删除 A，retain live nodes。
- A entry 不存在；B entry 仍命中。
- cache 长度从 2 降为 1，不能变为 0。

**Green：**

在节点生命周期收口处删除，或每次结构变化时 `retain_nodes`；禁止在每个 PTY output chunk 扫描整个 document。

**切片验证：**

```bash
cargo test -p app terminal_render_cache
cargo test -p app cards::term
cargo check -p app
bash scripts/check-performance-guardrails.sh
bash scripts/check-storage-boundaries.sh
```

---

### Task 6：P0/P1 完整验收

**预计：1 人日**

#### 自动化功能验证

```bash
cargo test -p runtime
cargo test -p app
cargo test -p e2e
cargo test -p e2e -- --ignored
cargo check -p app
bash scripts/check-performance-guardrails.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

域层依赖边界：

```bash
cargo tree -p terminal -p runtime | grep -i gpui
```

Expected：无输出。

#### L3 真实 App 验收

```bash
cargo build -p app --release
cargo packager -c Packager.dev.toml
bash scripts/run-cpu-performance-l3.sh baseline
bash scripts/run-cpu-performance-l3.sh candidate
```

使用 Computer Use 操作真实 App，逐项验证：

1. 创建 Terminal 和 Codex Agent。
2. 输入连续命令，确认回显无明显延迟或丢帧。
3. 运行两个持续刷新的 Codex 会话。
4. 观察工作蛇、呼吸光和光标仍连续。
5. 打开 Settings，确认 Voice Preview 内缘光仍保持实时反馈。
6. 改变主题、缩放、选择文本、滚动、resize，确认缓存正确失效。
7. 关闭节点，确认没有残影、旧 terminal frame 或缓存泄漏。

#### 调用栈验收

对 candidate 再运行 `/usr/bin/sample`：

- `shape_line`、`build_layout` 相对 baseline 明显下降。
- animation-only 场景不再重复出现终端 layout 构建。
- `Runtime::drain_all` 仍不是主要热点。
- 若 `bounds_tree`/`scene::insert_primitive` 成为剩余绝对热点，记录为 P2 入口证据，不在本计划扩 scope。

## 8. P0/P1 完成定义

以下全部满足才算完成：

- 所有新增 TDD 切片存在可复现 Red 记录和 Green 命令结果。
- Runtime/app/e2e 测试通过。
- Terminal/Agent 功能和动画视觉没有回归。
- 真实性能达到“相对 baseline 至少下降 50%”以及参考机目标；低于该门槛不算本计划完成。
- profile 中 terminal layout/shaping 热点显著下降。
- 未引入 GPUI 反向依赖、同步 SQLite、前台阻塞或无界缓存。
- P2/P3 没有夹带进本次实现。

## 9. 回滚策略

- 调度 gate 与 terminal render cache 分别保留独立开关，便于性能 A/B；开关只用于开发诊断，不进入长期用户设置。
- 若 repaint 合并导致输入延迟，先回退预算策略，不回退共享 snapshot。
- 若 shaped cache 出现视觉错误，保留 revision/Arc，临时回退到 cache miss 每帧 prepare，直到修正 key；不得用全局“主题变化清空所有状态”掩盖失效问题。
- 任一回滚都必须保留对应回归测试，并新增能复现问题的 Red。
