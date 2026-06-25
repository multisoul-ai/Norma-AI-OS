# Agent 卡片呼吸渐变边框 SPEC

## 1. 背景与目标

Agent（Codex / Claude Code）运行时由 `AgentTaskState` 驱动 `task_badge` 文字状态，但卡片边框本身始终是静态颜色，用户需要专门看左上角文字才能判断 agent 是否在工作、是否在等待自己批准。

本功能目标：让 Agent 卡片的边框在 agent 处于"运行中"或"等待用户批准"时呈现呼吸式色调循环动画，让用户从画布概览层面（无需聚焦到卡片细节）就能感知 agent 的活跃状态与是否需要介入。

## 2. 范围

### 2.1 In Scope

- 在 `NodeMotion`（`crates/app/src/motion.rs`）扩展每节点的呼吸动画相位状态。
- 仅 `NodeType::Agent` 卡片受影响；Terminal 卡片边框逻辑不变。
- 两套独立的色调循环 + 呼吸节奏，与 `AgentTaskState` 绑定：
  - `Thinking` / `RunningTool { .. }`：冷色组 `accent_cyan ↔ accent_violet ↔ accent_blue` 循环，呼吸周期 ≈2.5s。
  - `WaitingApproval { .. }`：暖色组 `accent_warm ↔ accent_orange` 循环，呼吸周期 ≈1.2s（更快，提示需要人工介入）。
- `Ready` / `Done` / `Failed { .. }` / `Ended` 时边框为现有静态 `stroke_color`，不参与呼吸；`Failed` 维持现有错误色高亮。
- 状态切换时直接切到新状态对应色组的循环起点，不做跨色组渐变过渡；进入 Done/Failed/Ended/Ready 时呼吸在 ~150ms 内淡出回静态色。
- 遵守现有 `MULTISOUL_REDUCE_MOTION=1` 约定：开启时呼吸动画关闭，边框直接使用 `agent_task_badge` 对应的静态颜色。
- 复用现有 `mix_color()` / `Hsla` / `AnimatedScalar` 基础设施与 12ms 帧循环（`motion.tick()`），不引入新渲染层、不依赖 GPUI 渐变背景 API。
- 为色相插值、呼吸 alpha 计算等纯函数补充单测（不依赖 GPUI 窗口）。

### 2.2 Out of Scope

- 不实现真正的空间旋转渐变描边（如 conic-gradient 风格的彩虹描边）；仅做时间维度的色调循环 + 透明度呼吸脉冲。
- 不改变 `task_badge` 文字/徽标的现有展示逻辑。
- 不为 Terminal 或其他卡片类型增加呼吸动画。
- 不新增 `RuntimeEvent` 或 hook 协议字段；完全基于现有 `AgentTaskState` 状态读取。
- 不引入跨色组的过渡动画（状态切换是硬切，不渐变混合两个色组）。
- 不修改 `canvas/terminal/runtime/storage` 的依赖方向约束。

## 3. 现状梳理（调研结论）

- Agent 卡片状态来自 `AgentTaskState`（`crates/runtime/src/events.rs`），由 `agent_hooks` HTTP 桥接（`crates/app/src/agent_hooks.rs` + `crates/runtime/src/agent_hooks.rs`）接收 Codex/Claude Code CLI 的 hook 事件并更新。
- 卡片当前用 `agent_task_badge()`（`crates/app/src/cards/agent/card.rs`）把 `AgentTaskState` 映射为 `CardChromeTaskBadge` 的色调（muted/active/warning/error），边框本身复用 `motion.rs` 的 `border_is_emphasized` 静态 1px/2px 逻辑，颜色不随任务状态变化。
- `NodeMotion`（`crates/app/src/motion.rs`）已有逐节点 `AnimatedScalar`（hover/pressed/selected/focused/dragging/opacity）与 `mix_color()` 颜色插值工具，并由 `app.rs` 中 12ms 定时器驱动 `motion.tick()` → `cx.notify()`。
- GPUI 0.2.2 的 `border_color` 字段类型为 `Option<Hsla>`，只接受纯色；`linear_gradient()` 仅能用于 `bg()` 背景填充，不能直接作为边框色。因此边框上的"渐变"效果只能通过时间维度的颜色插值（呼吸）模拟，不做空间维度的旋转描边。

## 4. 动画机制设计

### 4.1 状态到动画参数的映射

| AgentTaskState | 呼吸激活 | 色组 | 呼吸周期 |
|---|---|---|---|
| `Thinking` | 是 | cyan ↔ violet ↔ blue | ≈2.5s |
| `RunningTool { .. }` | 是 | cyan ↔ violet ↔ blue | ≈2.5s |
| `WaitingApproval { .. }` | 是 | warm ↔ orange | ≈1.2s |
| `Ready` | 否 | — | — |
| `Done` | 否 | — | — |
| `Failed { .. }` | 否（维持现有错误色） | — | — |
| `Ended` | 否 | — | — |

### 4.2 相位与颜色计算

- `NodeMotion` 新增 `breathing_phase: f32`（0.0~1.0 循环累加值），由 `motion.tick(dt)` 按当前状态对应周期推进：`phase += dt / period; phase %= 1.0`。
- 色相插值：用 `breathing_phase` 在色组的两到三个 `accent_*` 颜色间做分段 `mix_color()` 插值，构成连续循环（而非跳变）。
- 亮度呼吸：边框 alpha 按 `sin(phase * 2π)` 映射到一个区间（例如 0x66~0xFF），叠加在插值出的颜色上，形成"亮—暗—亮"的呼吸感。
- 以上计算封装为纯函数（如 `breathing_color_for_phase(phase, color_set) -> Hsla`），不依赖 GPUI 类型，可在 `crates/app` 内独立单测。

### 4.3 状态切换与退出

- 状态切换瞬间，`breathing_phase` 重置为该状态色组的循环起点（0.0），不做跨色组颜色混合过渡，保证状态边界在视觉上清晰可辨。
- 进入 `Done` / `Failed` / `Ended` / `Ready` 时，边框在约 150ms 内（参考现有 `hover_out`/`focus_selected` 量级的短过渡时长）从当前呼吸色淡出，回到 `agent_task_badge` 对应的静态色。

### 4.4 Reduce Motion

- 当环境变量 `MULTISOUL_REDUCE_MOTION=1` 时，跳过 `breathing_phase` 的帧推进与呼吸计算，边框直接渲染为 `agent_task_badge` 对应状态的静态颜色（即效果等同动画"提前定格"在某一帧的基准色，不闪烁）。

## 5. 验收标准

- Agent 卡片处于 `Thinking` 或 `RunningTool` 时，边框颜色在 cyan/violet/blue 之间连续循环呼吸，一个完整亮暗周期 ≈2.5 秒。
- Agent 卡片处于 `WaitingApproval` 时，边框颜色在 warm/orange 之间循环呼吸，周期 ≈1.2 秒，明显比运行中状态更急促。
- Agent 卡片处于 `Ready` / `Done` / `Failed` / `Ended` 时，边框为静态颜色，无呼吸动画；`Failed` 保持现有错误色高亮。
- 设置 `MULTISOUL_REDUCE_MOTION=1` 后，以上所有"运行中"/"等待批准"状态下边框均为静态色，不再呼吸。
- Terminal 卡片在任何情况下边框表现与本功能改动前一致。
- 状态切换（例如 `Thinking` → `WaitingApproval`）后，边框立即切换到新状态对应色组与节奏，不出现旧色组残留或跨色混合的瞬间。

## 6. 测试要求

- 按项目规范（见 `CLAUDE.md` 第 7 节），为 `breathing_color_for_phase` 等纯函数编写单测，覆盖：
  - 相位 0.0 / 0.25 / 0.5 / 0.75 等关键点的颜色与 alpha 计算是否符合预期插值公式。
  - 不同 `AgentTaskState` 到色组/周期的映射是否正确（含未激活状态返回静态色路径）。
  - `MULTISOUL_REDUCE_MOTION=1` 时是否完全跳过相位推进。
- 验证矩阵：本功能改动集中在 `crates/app/src/motion.rs` 与 `crates/app/src/cards/agent/`，按 `CLAUDE.md` 第 6 节要求执行 `cargo check -p app` 与相关单测；不涉及 `canvas`/`terminal`/`runtime`/`storage` 的对外 API 变更。

## 7. 动画原理说明（供实现完成后向用户解释）

这个呼吸渐变边框，本质上不是真正意义上的"沿边框轮廓旋转的渐变描边"——GPUI 0.2.2 的边框颜色字段只接受单一纯色（`Hsla`），不支持像 CSS `conic-gradient` 那样让颜色沿边框路径空间分布。

实际效果由两个时间维度的动画叠加而成：

1. **色调循环**：把"运行中"和"等待批准"分别映射到两组强调色（冷色组 cyan/violet/blue，暖色组 warm/orange），每帧根据一个 0~1 循环的相位值，在组内做颜色插值（`mix_color`），让边框颜色随时间连续地从一种颜色过渡到下一种，再循环回来。
2. **呼吸脉冲**：在颜色插值之上，再用一个正弦函数把边框透明度（alpha）按同一个相位值调制，产生"亮—暗—亮"的明暗起伏，这就是"呼吸"感的来源。

两者叠加后，整条边框在同一时刻仍然只是一种颜色（不是空间渐变），但这种颜色会随时间持续变化并伴随明暗起伏，因此视觉上呈现出"有生命感的渐变呼吸"效果，而不需要引入额外的渲染层或着色器开销——全部复用项目已有的 12ms 动画帧循环和颜色插值工具函数。
