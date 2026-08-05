# Norma OS CPU 优化 P2/P3 后续实施计划

> 状态：Deferred / Conditional  
> 日期：2026-07-25  
> 预计投入：P2 7～12 人日；P3 10～20 人日  
> 前置条件：先完成 P0/P1，并用正式 profile 证明剩余瓶颈确实位于全局 Canvas 重建或全行终端重绘。

## 1. 目标

本计划只处理 P0/P1 完成后仍存在的两个结构性瓶颈：

- P2：把可见卡片拆成独立 GPUI Entity，让一个节点变化只更新对应卡片，而不是重新 render 全部 Canvas 卡片。
- P3：使用 `alacritty_terminal 0.26` 的 damage 信息，只更新变化的终端行/列及对应 prepared rows。

这不是 P0/P1 的默认续作。满足阶段退出条件后必须重新 profile，只有收益证据充分才进入下一阶段。

## 2. 启动门槛

### P2 启动门槛

P0/P1 完成后，同时满足以下条件才启动 P2：

- 双 Codex 场景仍高于参考机 20% CPU，或相对 baseline 降幅不足 40%。
- profile 中 `render_canvas`、GPUI bounds tree、scene primitive 构建仍占主线程主要份额。
- terminal layout/shaping 已因 P1 cache 明显下降，证明剩余问题是“受影响卡片范围”，不是 cache key 错误。

### P3 启动门槛

P2 完成后，同时满足以下条件才启动 P3：

- 单个高速 TUI 仍导致目标节点本身 CPU 过高。
- profile 显示 terminal snapshot/prepare/paint 仍与总行数成正比。
- 实测多数帧只变化 1～3 行，但当前实现仍处理全部可见行。

不满足门槛时，本计划保持 Deferred，不为“架构更漂亮”而施工。

## 3. 范围边界

### P2 包含

- Runtime 返回具名 dirty NodeId 集合。
- `NodeCardView` Entity 生命周期、更新投影和局部 `cx.notify()`。
- Canvas 保持几何/viewport/选择/命中权威；卡片 Entity 只拥有渲染投影。
- Terminal、Agent、Preview、Browser、Rect 按纵向切片迁移。
- 卡片输入、焦点、drag、resize、关闭和 Browser native view 回归。

### P3 包含

- `terminal` 暴露 UI 中立的 Full/Partial damage。
- 共享 Terminal frame 支持按行结构共享或等价增量应用。
- app terminal cache 按 damaged rows 失效与重建。
- full snapshot 作为正确性 oracle 和恢复路径。

### 不包含

- 自研 Metal terminal renderer。
- scrollback 虚拟化产品功能。
- 改变 Terminal ANSI 语义。
- 更换 alacritty 或 GPUI 版本。
- 多窗口共享同一个 NodeCard Entity。

## 4. 实施前确认的测试 seam

实际施工前与用户确认：

1. **Runtime dirty report seam**：一次 `drain_all_report()` 返回精确 dirty NodeId 和 runtime events。
2. **NodeCard Entity seam**：输入 `NodeCardPresentation`；通过 Entity render/observe 证明只更新目标节点。
3. **Canvas 用户流程 seam**：公开 action、pointer、keyboard 和 Runtime 投影，验证迁移前后行为一致。
4. **Terminal damage seam**：`TerminalGrid::take_frame_update()` 返回 `Full` 或 `Partial`，可用完整 `snapshot()` 独立校验。
5. **增量 frame seam**：给定 base revision 和 delta，输出与完整 snapshot 完全相同的新 frame。
6. **真实性能 seam**：固定 L3 场景比较每帧重绘节点数、重建行数、CPU 和 WindowServer。

## 5. 架构约束

- `canvas`、`terminal`、`runtime` 继续零 GPUI。
- Runtime 仍是 Live Node、焦点、输入和任务状态的权威；NodeCard Entity 不复制业务状态机。
- `NodeCardView` 持有渲染所需的不可变 presentation，不在 render 中反查或修改整个 Runtime。
- Parent → child 使用强 `Entity`；child → parent 动作回传使用 `WeakEntity`，避免环。
- 所有 `Subscription` 必须保存在字段中。
- 禁止 Entity update 中嵌套 update 同一个或父 Entity；先计算投影，再分阶段 apply。
- Node 删除必须同时销毁 Entity、订阅、终端缓存和 native browser view。
- P3 damage 只是优化提示；revision 不匹配、resize、scroll、alternate screen 切换等情况必须安全回退 Full。

## 6. P2：节点级 Entity 与局部失效

### Task P2-0：冻结 P1 后基线

**预计：0.5 人日**

复用 P0/P1 runner，增加：

- 每个 emitted frame 的 root render 次数。
- 每个 NodeId 的 card render 次数。
- 每帧被重建的可见 card 数。
- 两个活跃 Agent 中只有 A 输出时，A/B 各自 render 次数。

必须先证明当前是 `A dirty → A/B/... 全部 render`，否则不进入 Entity 重构。

---

### Task P2-1：Runtime 返回精确 dirty NodeId

**预计：1～2 人日**

**修改：**

- `crates/runtime/src/runtime.rs`
- `crates/runtime/src/live.rs`
- `crates/runtime/tests/unit/runtime_tests.rs`
- `crates/app/src/app/frame_loop.rs`

建议契约：

```rust
pub struct RuntimeDrainReport {
    pub dirty_nodes: Vec<NodeId>,
    pub runtime_changed: bool,
}
```

`runtime_changed` 可以由 `!dirty_nodes.is_empty()` 与非节点状态变化组合得到，但不能丢失具名集合。

#### Cycle P2-1.1：单节点 dirty

**Red：**

`test_drain_report_names_only_terminal_that_produced_output`

- A/B 两个 Test PTY。
- 只向 A 注入 `"A1"`。
- `dirty_nodes` 精确等于 `[A]`。
- 不得包含 B。

**Green：**

Runtime 在现有 `for (node_id, node)` drain 时收集 NodeId；不得增加第二次全节点扫描。

#### Cycle P2-1.2：多节点合并与无重复

**Red：**

`test_drain_report_lists_two_dirty_nodes_once_each_in_document_order`

- A 收到两个 chunk，B 收到一个 chunk。
- report 各含 A/B 一次。
- C 未变化，不得出现。

**Green：**

一次 node.drain 的内部 chunk 数不泄漏到 dirty node 集合。

#### Cycle P2-1.3：status/event 兼容

分别测试：

- 退出状态包含对应 NodeId。
- 无 dirty 时集合为空。
- 现有 terminal status → runtime event 行为不变。

**验证：**

```bash
cargo test -p runtime drain_report
cargo test -p runtime
```

---

### Task P2-2：建立 NodeCardPresentation 与 Entity 生命周期

**预计：2～3 人日**

**新增/修改：**

- 新增 `crates/app/src/cards/node_view.rs`
- 新增 `crates/app/tests/unit/cards/node_view_tests.rs`
- 修改 `crates/app/src/app/mod.rs` / `bootstrap.rs`
- 修改 `crates/app/src/canvas/view.rs`

`NodeCardPresentation` 只包含渲染输入：

- NodeId/type/bounds。
- selected/focused/attention/task badge。
- current `NodeView`/revision。
- theme/zoom/terminal metrics。
- animation presentation。
- browser/preview 的稳定显示状态。

#### Cycle P2-2.1：创建与复用

**Red：**

`test_node_card_store_creates_one_entity_per_live_canvas_node_and_reuses_it`

- Document 有 A/B。
- 连续 reconcile 两次。
- A/B EntityId 分别稳定。
- 第二次不得创建第三个 Entity。

**Green：**

实现 `NodeCardStore<NodeId, Entity<NodeCardView>>` 的最小 reconcile。

#### Cycle P2-2.2：删除精确释放

**Red：**

`test_node_card_store_removes_closed_node_without_recreating_survivor`

- 删除 A。
- A Entity 不存在，B EntityId 不变。
- terminal cache/native browser cleanup 通过明确 hook 触发一次。

**Green：**

Entity 生命周期跟随 Document 结构变化，不跟随普通 repaint。

#### Cycle P2-2.3：弱回引用与订阅生命周期

用 headless GPUI 验证：

- child action 能通过 `WeakEntity<MultisoulApp>` 到达现有输入/关闭路径。
- parent 销毁后 weak upgrade 返回 None，不 panic。
- subscription 保存后能收到事件；节点删除后不再收到。

---

### Task P2-3：Terminal 卡片作为第一个局部失效 tracer bullet

**预计：2～3 人日**

只迁移 Terminal，先不批量迁移所有卡片。

#### Cycle P2-3.1：只更新目标 Terminal

**Red：**

headless GPUI 测试
`test_terminal_a_output_notifies_terminal_a_entity_without_rerendering_terminal_b`：

- A/B 两个 Terminal Entity 已完成首次 render。
- 只向 A 注入输出并应用 drain report。
- A presentation revision 前进，A render count `+1`。
- B revision 与 render count 均不变。
- root/Canvas 不得因纯终端内容变化重新构造 B。

render count 只通过 `#[cfg(test)]` 诊断快照暴露，属于本阶段明确性能契约。

**Green：**

- app frame loop 将 A 的最新 presentation apply 到 A Entity，并在 A 的 Context 内 `cx.notify()`。
- 纯终端内容变化不通知根 `MultisoulApp`。

#### Cycle P2-3.2：Canvas 全局变化仍更新全部必要卡片

分别做 Red/Green：

- zoom 改变 → 所有可见卡片 metrics/position 更新。
- theme 改变 → 所有卡片 theme presentation 更新。
- viewport pan → 只更新位置/可见集合，不改 terminal revision。
- selection/focus 从 A 到 B → A/B chrome 更新，C 不变。

不得为了让测试变绿而恢复每个 frame 的根级全量 presentation rebuild。

#### Cycle P2-3.3：输入与 resize

L1 e2e：

- 聚焦 A 输入只到 A PTY。
- resize A 只更新 A 的 grid metrics/cache。
- B 内容、焦点和 EntityId 不变。

---

### Task P2-4：按卡片类型逐个迁移

**预计：2～4 人日**

每种类型独立 Red/Green，顺序：

1. Agent：Terminal body + task badge + working snake。
2. Preview：加载/成功/失败状态。
3. Browser：chrome 与 native WebView bounds/visibility。
4. Rect：最小静态卡片。

每种类型至少验证：

- 自身状态变化只 notify 自己。
- Canvas zoom/theme/selection 能正确投影。
- 关闭后 Entity 和外部资源释放。
- 其他卡片 render count 不变。

Agent 特别要求：

- 工作蛇 phase 更新只重绘目标 Agent Entity。
- 同一个 Agent 的 cached terminal prepared frame 继续命中。
- Agent task Done/Failed 后动画停止，PTY 仍可存活。

Browser 特别要求：

- native view bounds 仍与 card body 对齐。
- 卡片被裁出 viewport 或 Settings 打开时隐藏。
- 删除 Browser 必须释放 native view，不能由 Entity 强引用泄漏。

---

### Task P2-5：P2 用户流程与性能验收

**预计：1～2 人日**

自动化：

```bash
cargo test -p runtime
cargo test -p app
cargo test -p e2e
cargo test -p e2e -- --ignored
cargo check -p app
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

必须覆盖既有：

- Terminal 创建、输入、回显、scrollback、resize、关闭。
- Agent cursor/working snake/task 状态。
- Preview 与 Browser。
- Canvas drag、pan、selection、focus、reflow。
- Settings 遮挡与 Voice Preview。

使用 Computer Use 操作真实 App 验收所有卡片类型、拖拽、缩放、焦点和动画视觉。

P2 退出门槛：

- `A dirty → B render` 的诊断计数为 0。
- 多卡片场景 GPUI bounds/scene 构建热点相对 P1 baseline 明显下降。
- 双 Codex 中位 CPU 达到目标，或再下降至少 30%。
- 若剩余热点主要是单个 Terminal 的全行处理，才进入 P3。

## 7. P3：Terminal dirty-row 增量帧与绘制

### Task P3-0：验证 alacritty damage 契约

**预计：0.5～1 人日**

`alacritty_terminal 0.26` 已提供：

- `Term::damage() -> TermDamage::{Full, Partial}`
- `LineDamageBounds { line, left, right }`
- `Term::reset_damage()`

时间盒 0.5 人日验证：

- `damage()` 会额外包含旧/新 cursor damage。
- display offset、resize、alternate screen 等场景何时返回 Full。
- 必须在复制 damage 后调用 `reset_damage()`。

只写结论到计划进度，不修改依赖版本。

---

### Task P3-1：UI 中立的 TerminalFrameUpdate

**预计：2～4 人日**

**修改：**

- `crates/terminal/src/grid.rs`
- 新增 `crates/terminal/src/grid/damage.rs`
- 新增/修改 `crates/terminal/src/grid/*` 测试
- `crates/terminal/src/session.rs`

建议领域契约：

```rust
pub enum TerminalFrameUpdate {
    None,
    Full {
        revision: u64,
        snapshot: GridSnapshot,
    },
    Partial {
        base_revision: u64,
        revision: u64,
        rows: Vec<DamagedRow>,
        cursor: CursorSnapshot,
        modes: TerminalModeSnapshot,
    },
}
```

`DamagedRow` 使用中性 `Cell`，不暴露 alacritty 类型。

#### Cycle P3-1.1：初始帧必须 Full

**Red：**

`test_initial_terminal_frame_update_is_full_and_covers_all_rows`

- 80×24，完整行数为 24。
- 初始不能返回 Partial/None。

**Green：**

初始化 damage 转为 Full，建立 revision。

#### Cycle P3-1.2：单行输出返回 Partial

**Red：**

`test_single_line_text_update_reports_named_damaged_row_and_cursor`：

- reset 初始 damage。
- 输入不会滚屏的短文本 `"A"`。
- Partial 必须包含第 0 行和正确 Cell。
- 不得包含未变化的远端行，例如第 23 行。
- cursor 信息同步更新。

**Green：**

复制 `LineDamageBounds` 对应的中性 cells 后 reset damage。

#### Cycle P3-1.3：结构变化安全回退 Full

逐个 Red/Green：

- resize 返回 Full。
- scrollback display offset 改变返回 Full。
- alternate screen enter/leave 返回 Full。
- clear screen/scroll region 若 alacritty 报 Full，保持 Full，不擅自缩窄。

**验证：**

```bash
cargo test -p terminal damage
cargo test -p terminal
```

---

### Task P3-2：Runtime 安全应用增量 frame

**预计：2～4 人日**

**修改：**

- `crates/runtime/src/nodes/term.rs`
- `crates/runtime/src/live.rs`
- `crates/runtime/tests/unit/runtime_tests.rs`

为了让未变化行结构共享，允许把 frame rows 演进为等价结构：

```text
Arc<[Arc<[Cell]>]>
```

但对外仍提供稳定、UI 中立的读取接口；不要让 app 依赖内部容器。

#### Cycle P3-2.1：Partial 与完整 snapshot 等价

**Red：**

`test_partial_frame_application_matches_full_snapshot_oracle`：

- 从同一初始 frame 分叉。
- 一支应用 Partial；另一支直接读取完整 `snapshot()`。
- rows、cursor、modes、display offset 逐项相等。
- 未变化行 Arc 身份保持；变化行身份必须不同。

**Green：**

只复制被 damage 命中的 rows；其他 row Arc 复用。

#### Cycle P3-2.2：base revision 不匹配拒绝增量

**Red：**

`test_partial_frame_with_stale_base_revision_requests_full_refresh`：

- current revision=10。
- delta base_revision=9。
- 返回明确 `NeedsFullRefresh`。
- frame 内容不得被部分修改。

**Green：**

失败原子化；不得“尽量应用”造成混帧。

#### Cycle P3-2.3：Full 恢复

验证 base mismatch 后下一次 Full 能恢复，revision 单调且最终内容与 oracle 一致。

---

### Task P3-3：Terminal render cache 按行失效

**预计：3～5 人日**

**修改：**

- `crates/app/src/cards/term/render_cache.rs`
- `crates/app/src/cards/term/paint.rs`
- 对应 app 单测

缓存从“每 Node 一整帧”演进为：

```text
Node entry
  ├── row 0 prepared layout/shaped runs
  ├── row 1 prepared layout/shaped runs
  └── ...
```

#### Cycle P3-3.1：单行 damage 只重建一行

**Red：**

`test_terminal_render_cache_reprepares_only_named_damaged_row`：

- 24 行初始 frame 已全部 prepared。
- 下一帧只 damage 第 5 行。
- prepare count 增加 1。
- 第 5 行 prepared identity 变化。
- 第 4/6 行 identity 保持。

**Green：**

只失效 damage rows；cursor 单独绘制，不把旧/新 cursor 行强制文字重排。

#### Cycle P3-3.2：Full damage 重建全部行

resize/theme/font metrics 变化必须重建全部 24 行；负断言不能只重建一行。

#### Cycle P3-3.3：selection 只影响相交行

selection 从第 3 行到第 4 行，只使相关 selection geometry/paint 更新；未相交文本 shaping 继续复用。

---

### Task P3-4：增量路径与完整 oracle 的协议回放

**预计：2～3 人日**

新增确定性协议回放测试。每个 ANSI chunk 后同时：

1. 获取完整 `GridSnapshot` oracle。
2. 获取并应用 `TerminalFrameUpdate`。
3. 比较所有 rows、cursor、status 和 modes。

至少覆盖：

- 普通 UTF-8 文本。
- 16 色、256 色、truecolor 与 reset。
- cursor movement/save/restore。
- erase line/display。
- insert/delete character。
- 滚屏和 scroll region。
- alternate screen enter/leave。
- OSC 8 hyperlink。
- resize。
- 宽字符与 spacer。

每个测试使用固定字节序列和已知最终文字做正/负断言，不能只断言“增量结果等于同一实现生成的增量结果”。

Run:

```bash
cargo test -p terminal frame_update
cargo test -p runtime partial_frame
cargo test -p app terminal_render_cache
cargo test -p e2e e2e_02_terminal
```

---

### Task P3-5：P3 真实性能与视觉验收

**预计：1～2 人日**

固定场景：

- 80×24 普通 shell 单行 spinner。
- 160×50 Codex TUI 状态行更新。
- 高频日志滚屏。
- alternate screen 全屏刷新。

记录：

- Full/Partial/None frame 数。
- 每帧 damaged rows 数。
- prepared rows 数。
- shaped runs 数。
- Node Entity render 次数。
- Norma OS/WindowServer CPU。

P3 退出门槛：

- 单行 spinner 场景每帧平均 prepared rows `≤ 3`。
- 完整 snapshot oracle 与增量 frame 全部一致。
- 单 TUI 场景相对 P2 baseline CPU 再下降至少 25%，否则评估 P3 复杂度是否值得保留。
- full-damage 场景没有比 P2 明显退化。
- Computer Use 验收真实 TUI 没有残字、颜色错位、cursor ghost、滚屏撕裂或旧 alternate screen 内容。

## 8. 完整验证矩阵

```bash
cargo test -p terminal
cargo test -p runtime
cargo test -p app
cargo test -p e2e
cargo test -p e2e -- --ignored
cargo test --workspace
cargo check -p app
cargo build -p app --release
bash scripts/check-performance-guardrails.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

依赖边界：

```bash
cargo tree -p terminal -p runtime | grep -i gpui
```

Expected：无输出。

## 9. 风险与回滚

| 风险 | 防护 | 回滚 |
|---|---|---|
| Entity 强引用环导致节点不释放 | child → parent 只用 WeakEntity；生命周期测试 | 回退对应卡片类型迁移 |
| 嵌套 update 触发 GPUI borrow panic | presentation 先计算、后分阶段 apply | 恢复根级该类卡片更新 |
| Browser native view 与 Entity bounds 脱节 | Browser 专用 L1/L3 测试 | Browser 暂留根渲染 |
| dirty damage 漏报导致残字 | 完整 snapshot oracle；revision mismatch 回退 Full | 关闭 Partial，保持 Full |
| row cache key 不完整 | theme/metrics/selection/full-damage 矩阵 | 回退整 Node cache |
| 增量复杂度收益不足 | 每阶段真实性能退出门槛 | 删除 P3 Partial 路径，保留 P2 |

## 10. P2/P3 完成定义

- P2 与 P3 分别通过自己的启动门槛和退出门槛，不因计划存在而默认实施。
- 每个纵向切片保留 Red/Green 证据，测试满足项目完整注释和断言规范。
- 一个节点的 PTY/动画变化不重渲染其他卡片。
- Partial frame 与完整 snapshot oracle 在协议矩阵中完全一致。
- 所有真实 Terminal/Agent/Canvas/Browser 用户流程无回归。
- profile 中的热点与预期一致；若收益不足，按回滚策略停止，不继续追加沉没成本。

