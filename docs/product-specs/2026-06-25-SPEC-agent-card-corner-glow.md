# Agent 卡片四角光晕 SPEC

## 1. 背景与目标

[[2026-06-24-SPEC-agent-card-breathing-border]] 已经让 Agent 卡片在运行中 / 等待批准时，边框颜色按冷色组 / 暖色组循环呼吸。但效果只是单色随时间变化的描边，不具备用户提供的 Uiverse CSS 参考效果（`m2` 卡片）里"四角多彩光晕 + 模糊呼吸"的视觉层次——四个角各自带一团固定颜色的光晕（橙/蓝/粉/紫），整体随时间在锐利与朦胧之间呼吸。

本功能目标：在现有单色呼吸描边**之上**叠加一层四角彩色光晕，让 Agent 卡片在运行中的"活跃感"更接近参考效果，同时不改变也不删除已有的呼吸描边机制与测试。

## 2. 范围

### 2.1 In Scope

- 在 `crates/app/src/motion_breathing.rs` 新增 4 个固定的角光晕颜色常量（还原参考 CSS 的橙 / 蓝 / 粉 / 紫，不随主题、不随 Cool/Warm 状态切换颜色）。
- 在 `crates/app/src/motion.rs` 的 `ResolvedNodeVisual` 新增 `corner_glow: Option<CornerGlow>` 字段：
  - `CornerGlow { blur: f32, alpha: f32 }`：模糊半径与整体强度，颜色不放在这里（颜色是 4 个固定常量，在渲染层直接取用）。
  - 仅当节点是 Agent 且 `breathing_display_state != Inactive` 且 `breathing_fade.value > EPSILON` 时为 `Some`；其余情况为 `None`。
  - `alpha` 由现有 `breathing_fade.value`（淡入淡出）与一个新的呼吸包络函数 `corner_glow_envelope(phase) -> f32`（映射到 0.0~1.0，复用 `sin(phase * 2π)` 思路，与现有 `breathing_alpha` 共享同一个 `phase` 输入，避免重复三角函数调用）相乘得到。
  - `blur` 由同一个 `phase` 在 8px~14px 之间波动（对应参考 CSS `blur-animation` 的 2vmin↔3vmin），复用呼吸周期（Cool ≈2.5s / Warm ≈1.2s），不引入新计时器。
- 在 `crates/app/src/cards/chrome.rs::node_shadow` 中，当 `visual.corner_glow` 为 `Some` 时，向返回的 `Vec<BoxShadow>` 追加 4 个固定颜色（橙/蓝/粉/紫）的 `BoxShadow`，`offset` 分别指向左上 / 右上 / 左下 / 右下角（按 `zoom` 缩放的固定像素偏移），`blur_radius` 取 `corner_glow.blur`，颜色 alpha 字节按 `corner_glow.alpha` 调制。
- 性能约束（见第 4 节）：零额外定时器、`None` 路径零分配零绘制、按屏幕尺寸裁剪、共享包络值、`Vec` 预分配容量。
- Terminal 卡片、非 Agent 卡片完全不受影响。
- 为新增纯函数（角光晕包络、blur 波动）补充单测，覆盖关键相位点与 Inactive/Done 收尾路径。

### 2.2 Out of Scope

- 不实现真正的 radial-gradient 背景或 backdrop blur（GPUI 0.2.2 不支持，仅 2-stop `linear_gradient` 背景与解析高斯 `BoxShadow`）。
- 不让四角光晕颜色随 Cool/Warm 状态切换（用户明确要求"完全复制原 CSS 四色"，颜色固定，只有呼吸节奏快慢随状态变化）。
- 不修改 `.m2` 参考 CSS 里卡片背景自身的 `gradient-shift` 动画（本功能只做边框/光晕，不碰卡片填充背景）。
- 不改变现有单色呼吸描边（`stroke_color`）的颜色循环逻辑与既有测试断言。
- 不修改 `canvas` / `terminal` / `runtime` / `storage` 的依赖方向约束。

## 3. 现状梳理（调研结论）

- `motion.rs::NodeMotion` 已维护 `breathing_phase: f32`（0.0~1.0 循环）、`breathing_display_state: BreathingState`、`breathing_fade: AnimatedScalar`，由 `motion.tick(dt)` 在现有 12ms 帧循环中推进，详见 [[2026-06-24-SPEC-agent-card-breathing-border]] §4.2。本功能直接复用这三个量，不新增计时器。
- `chrome.rs::node_shadow(visual) -> Vec<BoxShadow>` 当前返回 1~2 个 `BoxShadow`（基础阴影 + Focused 态的强调光晕），结构上已经支持"追加更多 shadow"。
- GPUI 0.2.2 的 `BoxShadow { color: Hsla, offset: Point<Pixels>, blur_radius: Pixels, spread_radius: Pixels }` 在 Metal/Blade 渲染后端中用单 pass 解析高斯近似（`erf` 积分）计算模糊，每个 shadow 是一次 GPU quad 绘制，不需要离屏多次模糊；成本与该 shadow 覆盖的像素面积近似线性相关，不随 `blur_radius` 增大产生额外 pass。这意味着叠加 4 个小尺寸 shadow 的 GPU 开销可忽略，性能瓶颈主要在 CPU 侧的逐帧分配与计算。
- `canvas_view.rs` 渲染节点时已有 viewport culling（节点完全在窗口外则 `continue` 跳过整段渲染），但没有"节点过小则跳过细节效果"的裁剪。

## 4. 动画与性能机制设计

### 4.1 固定四色

| 角位 | 颜色 | 近似 RGB | 来源 |
|---|---|---|---|
| 左上 | 橙 | `#F8903A` | 还原 CSS `hsl(27deg 93% 60%)` |
| 右上 | 蓝 | `#00A6FF` | 还原 CSS `#00a6ff` |
| 左下 | 粉 | `#FF0056` | 还原 CSS `#ff0056` |
| 右下 | 紫 | `#6500FF` | 还原 CSS `#6500ff` |

四色为固定常量，不随主题（亮/暗）、不随 `BreathingState::Cool`/`Warm` 切换；`Cool`/`Warm` 只通过既有 `period()`（2.5s / 1.2s）影响呼吸节奏快慢。

### 4.2 触发条件

`corner_glow` 仅在以下条件全部满足时为 `Some`：

1. 节点 `NodeType::Agent`。
2. `breathing_display_state != BreathingState::Inactive`（即 `Cool` 或 `Warm`）。
3. `breathing_fade.value > EPSILON`（与现有单色呼吸描边共享同一套淡入淡出节奏，进入 `Done`/`Failed`/`Ended`/`Ready` 时随 `breathing_fade` 一起在 ~150ms 内淡出到 `None`，不单独引入新的过渡时长）。
4. `MULTISOUL_REDUCE_MOTION=1` 时：与现有呼吸描边一致，直接判定为 `None`（不产生光晕，不产生帧推进开销）。

### 4.3 强度与模糊的计算

- 复用 `breathing_phase`，新增一个纯函数：

  ```rust
  fn corner_glow_envelope(phase: f32) -> f32 {
      let s = (phase * TAU).sin();
      (s + 1.0) / 2.0 // 0.0..=1.0
  }
  ```

  与现有 `breathing_alpha`（映射到 `0x66..=0xFF`）结构一致，但独立成另一个返回 `f32` 的函数，供 `blur` 与 `alpha` 共同复用同一次 `sin` 调用结果（在 `motion.rs` 里算一次，传给两处，不重复调用三角函数）。
- `corner_glow.alpha = breathing_fade.value * envelope`（`envelope` 即上面函数结果）。
- `corner_glow.blur = lerp(8.0, 14.0, envelope)`（像素，未乘 zoom；渲染层按 `zoom` 缩放）。

### 4.4 渲染层叠加

`node_shadow` 追加的 4 个 `BoxShadow`：

- `offset`：以卡片中心为参照，分别向左上 / 右上 / 左下 / 右下偏移固定基准距离（例如 6px，按 `zoom` 缩放），让光晕视觉上偏向对应角，模拟参考 CSS 四个 `radial-gradient(circle at <corner>, color, transparent)`。
- `blur_radius`：`corner_glow.blur * zoom`。
- `spread_radius`：固定小负值（如 `-2px * zoom`），避免实色边缘过硬。
- `color`：4.1 节固定色，alpha 字节按 `corner_glow.alpha` 调制（`0..=1.0` 映射到 `0x00..=0xFF`，再与原始颜色的不透明强度相乘，避免四色同时叠加时过曝）。

### 4.5 性能约束

- **零额外定时器**：完全复用 `breathing_phase`/`breathing_fade` 已有的逐帧推进；不新增 `Duration`/计时状态。
- **`None` 路径零开销**：非 Agent 节点、Agent 未激活呼吸、或 reduce-motion 开启时，`corner_glow` 为 `None`，`node_shadow` 不进入四角光晕分支，不分配、不绘制，与改动前行为一致。
- **屏幕尺寸裁剪**：在 `canvas_view.rs` 现有 viewport culling 之外，新增条件——当节点屏幕尺寸（`w`/`h` 乘以 `zoom` 后）小于阈值（建议 32px）时，即使 `corner_glow` 为 `Some` 也不追加四角 shadow（视觉上不可感知，避免缩小视图下多张小卡片同时绘制不可见光晕）。
- **共享包络值**：`corner_glow_envelope(phase)` 只算一次，`alpha` 与 `blur` 都复用该值，4 个角的 shadow 也复用同一对 `(alpha, blur)`，只是 `offset`/颜色不同。
- **`Vec` 预分配**：`node_shadow` 用 `Vec::with_capacity(6)`（2 个既有 + 4 个新增上限）构造返回值，避免 `push` 触发的多次扩容。
- GPU 侧依据第 3 节调研结论，4 个小尺寸 `BoxShadow` 的解析高斯渲染成本可忽略，无需额外优化。

## 5. 验收标准

- Agent 卡片处于 `Thinking` / `RunningTool` / `WaitingApproval` 时，卡片四角可见橙 / 蓝 / 粉 / 紫四团光晕，整体随呼吸相位在模糊范围（8~14px 等效）与强度上起伏，呼吸节奏与既有单色描边的 Cool/Warm 周期一致（2.5s / 1.2s）。
- 四角光晕颜色在 `Thinking`/`RunningTool`（冷色组描边）与 `WaitingApproval`（暖色组描边）之间**不变**（始终是固定的橙/蓝/粉/紫），只有节奏快慢不同。
- Agent 卡片处于 `Ready` / `Done` / `Failed` / `Ended` 时无四角光晕；从激活态切到这些状态时，光晕在 ~150ms 内随既有 `breathing_fade` 一起淡出，不残留。
- `MULTISOUL_REDUCE_MOTION=1` 时不出现四角光晕（与单色呼吸描边一致直接判定为静态）。
- Terminal 卡片、非 Agent 卡片在任何状态下渲染表现与本功能改动前一致。
- 画布缩小到节点屏幕尺寸低于裁剪阈值时，四角光晕不绘制；放大回阈值以上时光晕恢复，不产生闪烁或残留。
- 既有 [[2026-06-24-SPEC-agent-card-breathing-border]] 的验收标准与单测全部保持通过，未被本功能破坏。

## 6. 测试要求

- 按项目规范（`CLAUDE.md` 第 7 节），为 `corner_glow_envelope` 等新增纯函数编写单测，覆盖：
  - 相位 0.0 / 0.25 / 0.5 / 0.75 关键点的包络值是否符合 `sin` 公式推导。
  - `corner_glow` 在 Agent + Cool/Warm 激活时为 `Some`，在 Inactive/Done/Failed/Ended/Ready 及非 Agent 节点时为 `None`。
  - `MULTISOUL_REDUCE_MOTION=1` 时 `corner_glow` 恒为 `None`。
  - `blur` 在 8.0~14.0 范围内随相位波动，不越界。
- 验证矩阵：改动集中在 `crates/app/src/motion_breathing.rs`、`crates/app/src/motion.rs`、`crates/app/src/cards/chrome.rs`、`crates/app/src/canvas_view.rs`（屏幕尺寸裁剪）。按 `CLAUDE.md` 第 6 节执行 `cargo check -p app` 与相关单测；不涉及 `canvas`/`terminal`/`runtime`/`storage` 的对外 API 变更。

## 7. 与参考效果的对应关系（供实现完成后向用户解释）

参考的 Uiverse CSS（`.m2`）用 `::before`/`::after` 两层伪元素叠加四个 `radial-gradient(circle at <corner>, color, transparent)`，再用 `filter: blur(...)` 做模糊呼吸，本质是"空间上固定四角颜色 + 时间上模糊呼吸"的组合。

GPUI 0.2.2 没有 `radial-gradient` 背景，也没有 `backdrop-filter: blur`，因此无法 1:1 复刻该 CSS 实现路径。本功能用 4 个偏移指向四角、固定颜色、模糊半径随时间呼吸的 `BoxShadow` 来模拟同样的视觉效果：每个 `BoxShadow` 本身就是一个"以矩形为中心、向外扩散并模糊"的光晕，把它的 `offset` 推向某个角，效果上接近"该角更亮"的径向光晕；4 个角各放一个不同颜色的 shadow，叠加起来就形成了四角多彩光晕。而 `blur_radius` 随呼吸相位在 8~14px 之间波动，复刻了参考 CSS `blur-animation`（"模糊忽强忽弱"）的呼吸感。

这套实现完全复用了 [[2026-06-24-SPEC-agent-card-breathing-border]] 已经建好的呼吸相位与淡入淡出基础设施，没有引入新的渲染层、新的计时器或新的着色器，因此在保留视觉效果的同时，CPU/GPU 开销都被控制在"现有呼吸描边机制的边际成本"范围内。
