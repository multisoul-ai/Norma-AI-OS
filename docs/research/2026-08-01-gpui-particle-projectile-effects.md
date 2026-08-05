# GPUI Agent 任务派发粒子特效调研

> 调研日期：2026-08-01  
> 需求：从底部 Command Dock 发射一枚“流星”，平滑命中动态 Agent 卡片中心，并在命中时产生脉冲/爆炸反馈。  
> 范围：只做技术调研与落点建议，不实施代码。  
> 版本边界：Norma OS 当前钉死 `gpui = 0.2.2`、`gpui-component = 0.5.1`。

## 结论

推荐在 `crates/app` 内实现一个轻量的、短生命周期的 `DispatchEffect` 状态机，并用单个全窗口 GPUI `canvas` 特效层绘制。不要引入 Bevy、Macroquad、Hanabi、Enoki 或第二套 GPU/窗口运行时，也不建议为了这个效果引入第三方 `gpui-animation`。

原因：本需求同时存在的活跃对象很少，通常只有一枚飞行体、一个短尾迹和 12–18 个命中粒子。GPUI 0.2.2 已提供逐帧刷新、`canvas`、`PathBuilder`、`paint_path`、`paint_quad` 与 `paint_shadows`；Norma 也已有统一动画帧驱动和自定义路径绘制案例。完整粒子引擎解决的是成百上千粒子的批量模拟与渲染，对当前效果属于架构过量。

推荐的第一版：

1. 飞行阶段用三次 Bézier 轨迹，终点每帧从 `NodeId` 对应卡片的实时视觉矩形重新解析。
2. 尾迹保存最近 6–10 个头部位置，只构建一条短路径。
3. 命中阶段绘制 1–2 圈扩散描边、一个克制的光晕，以及 12–18 个短寿命四边形粒子。
4. 动画状态进入现有 app `MotionStore`/帧调度体系，无活动效果时零持续刷新。
5. `reduce_motion` 下取消跨屏飞行，只保留 Dock 瞬时亮起与目标卡片静态脉冲。

## GitHub / 生态搜索结果

### 1. Zed / GPUI 原生能力

GPUI 官方仓库没有通用粒子系统或烟花组件。它提供的是足以自行构建小型特效的底层原语：

- [`Animation` / `AnimationExt`](https://github.com/zed-industries/zed/blob/69e2130295c2649963eb639fc70b4f2ee8ea1624/crates/gpui/src/elements/animation.rs)：按时间输出 `0..1` 进度，动画未结束时请求下一帧。
- [`canvas`](https://github.com/zed-industries/zed/blob/69e2130295c2649963eb639fc70b4f2ee8ea1624/crates/gpui/src/elements/canvas.rs)：提供低层 `prepaint` / `paint` 回调，官方定位就是短期自定义绘制。
- [`PathBuilder`](https://github.com/zed-industries/zed/blob/69e2130295c2649963eb639fc70b4f2ee8ea1624/crates/gpui/src/path_builder.rs)：支持直线、二次/三次曲线和圆弧；`build()` 使用 Lyon 在 CPU 端做填充/描边 tessellation。
- [`Window`](https://docs.rs/gpui/0.2.2/gpui/struct.Window.html)：公开 `request_animation_frame`、`paint_path`、`paint_quad` 与 `paint_shadows`。

GPUI 官方 README 把低层 Element 描述为可用于高效自定义布局与绘制的接口，同时明确 GPUI 仍处于 pre-1.0、版本之间可能破坏兼容。因此方案应严格依赖本仓库已验证的 0.2.2 API，而不是照搬 Zed `main` 的新写法。[官方说明](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)

`gpui-component 0.5.1` 的现有动效也建立在 GPUI 原生能力上，例如 Spinner 用重复 `Animation` 旋转 SVG，Dialog 用 `with_animation` 修改位置、阴影和透明度。仓库和本地 0.5.1 源码扫描没有发现 particle/confetti/firework/projectile emitter。[Spinner 源码](https://github.com/longbridge/gpui-component/blob/0f0ab35233212f8f3277028995caf0c41e13ee6c/crates/ui/src/spinner.rs)、[Dialog 源码](https://github.com/longbridge/gpui-component/blob/0f0ab35233212f8f3277028995caf0c41e13ee6c/crates/ui/src/dialog.rs)

判断：GPUI 原生 API 足够，但需要 Norma 自己维护效果状态与绘制；生态里没有可直接插入的官方粒子组件。

### 2. `gpui-animation`

第三方 [`chi11321/gpui-animation`](https://github.com/chi11321/gpui-animation) 直接依赖 `gpui ^0.2.2`，提供元素样式的状态过渡、优先级、颜色/尺寸/阴影插值，并宣称无活动时停驻后台任务。

它不适合作为本效果的核心：

- README 明确标记为 early development，API 可能变化。
- 抽象中心是 `StyleRefinement` 与 hover/click/declarative transition，不是动态目标、轨迹采样或粒子发射器。
- 对 12–18 个粒子分别建立 GPUI 元素和样式过渡，会比单 `canvas` 绘制产生更多元素、布局和状态开销。
- Norma 已有自己的动画帧驱动，再引入一个独立 tick/任务体系会重复职责。

它最多可作为普通控件状态过渡的未来候选，不建议因本需求新增依赖。

### 3. Rust 粒子 / 烟花实现

| 项目 | 实现特点 | 可借鉴内容 | 不直接采用的原因 |
|---|---|---|---|
| [Macroquad particles](https://github.com/not-fl3/macroquad/tree/master/particles) | CPU 更新粒子，GPU 实例数据；支持 one-shot、lifetime、spread、velocity、gravity、颜色/尺寸曲线和 additive blend | 小型 `Particle`/`EmitterConfig` 数据模型；命中爆炸的 lifetime、速度、扩散角和衰减设计 | 依赖 Macroquad 的 GL 上下文、主循环和渲染后端，无法自然嵌入 GPUI；[官方爆炸示例](https://github.com/not-fl3/macroquad/blob/master/examples/particles_example.rs) |
| [bevy_enoki](https://github.com/Lommix/bevy_enoki) | CPU 模拟 + GPU instancing，面向 2D、WASM 和移动端；支持随机参数、drag、gravity、scale/color curves | “CPU 做少量状态、GPU 批量绘制”的分层；曲线化的大小/颜色生命周期 | 是 Bevy 插件，要求 Bevy ECS、资产和渲染栈；本需求粒子数量远不足以抵消集成成本 |
| [Bevy Hanabi](https://github.com/djeedai/bevy_hanabi) | GPU compute 粒子；支持 burst、力场、拖曳、颜色/尺寸生命周期、trails/ribbons 和大量粒子 | 如果未来要做持续环境 VFX，可参考其 spawn/init/update/render 分阶段模型 | 依赖 Bevy + compute shader，项目也提示仍在重度开发；对十几个 UI 粒子明显过量 |
| [Macroquad](https://github.com/not-fl3/macroquad) / [Comfy](https://github.com/darthdeus/comfy) | 完整 2D 游戏引擎，拥有自己的事件循环、相机、输入和绘制批处理 | 可参考几何批处理、粒子池与 trail 数据结构 | 会在 GPUI 内引入第二套应用/渲染生命周期，不应作为 UI 特效依赖 |

共同可复用的不是库本身，而是数据模型：`spawn -> initialize -> update(dt) -> render -> retire`，以及每粒子的 `position / velocity / age / lifetime / color / size`。

## 推荐技术设计

### 状态与职责

建议状态机：

```text
Accepted receipt
      ↓
Flight (约 220–300ms)
      ↓ distance / t reaches target
Impact (约 140–180ms)
      ↓ all particles expired
Retired
```

`DispatchEffect` 最少包含：

- `effect_id` / `task_id`：去重、诊断与测试。
- `target_node_id`：不要只保存一次性的目标坐标。
- `phase`、`elapsed`、`flight_duration`。
- `start_screen`：发射瞬间的 Dock 中心。
- `filtered_target_screen`：动态目标的平滑位置。
- `trail_samples`：固定容量环形缓冲。
- `impact_particles`：固定上限的小数组，方向与速度由稳定 seed 生成。

建议放置：

- 状态与纯时间推进：`crates/app/src/motion/dispatch_effect.rs`，由 `MotionStore` 持有。
- 特效层绘制：`crates/app/src/app/chrome/dispatch_effect.rs`，在画布/card 之上、Command Dock 之下或与 Dock 同级。
- 触发：Agent dispatch receipt 已经确定为接受状态之后，而不是用户刚提交命令时。

不要放入 `crates/canvas`：该效果依赖 GPUI、窗口坐标、Command Dock 和 Agent receipt，是 app 呈现行为，不是 Canvas 域模型。

### 起点与动态目标

起点可由当前窗口尺寸和 Dock 固定布局解析：Dock 水平居中，`bottom = 22px`、`height = 52px`，因此中心约为 `(viewport_width / 2, viewport_height - 48px)`。若未来 Dock 可移动或改变高度，再增加实际 bounds 回传；第一版无需引入通用锚点系统。

目标每帧按以下顺序解析：

1. 由 `target_node_id` 取得节点 canonical rect。
2. 取得 `MotionStore` 当前的 `ResolvedNodeVisual.rect`，而不是未动画的 document rect。
3. 用当前 viewport 的 `canvas_to_screen` 转换视觉矩形中心。
4. 对目标位置做帧率无关低通平滑：`filtered += (live - filtered) * (1 - exp(-k * dt))`。
5. 飞行结束的最后一帧强制以实时中心为命中点，避免滤波造成“差一点没打中”。

卡片拖动、布局动画、画布平移和缩放都会改变目标屏幕坐标；以上做法能持续追踪，而保存一次性 endpoint 做不到。

目标卡片在飞行中被删除时，效果应在当前位置快速淡出并 retire；不要飞向旧坐标。目标完全离屏时，建议不播放跨屏飞行，只做 Dock 确认或边缘提示，避免一个看不见的命中过程持续占帧。

### 飞行轨迹

推荐三次 Bézier，而不是刚体弹道或无限趋近的弹簧：

```text
P0 = Dock 中心
P1 = P0 向上偏移，保证明确的“发射”方向
P3 = 每帧更新的目标卡片中心
P2 = P3 沿入射反方向偏移，保证流星朝目标收束
head = cubic_bezier(P0, P1, P2, P3, eased_t)
```

因为三次 Bézier 中终点对当前位置的影响在前段较弱、后段更强，实时更新 `P3` 天然形成“后半程制导”；再加目标低通即可避免卡片轻微抖动传到流星。时间曲线宜使用强 `ease-out` 或接近匀速的自定义曲线，确保派发后立即看到响应；总飞行时间建议不超过 300ms。

尾迹不需要连续发射大量粒子。保存最近 6–10 帧头部位置，构建一条短折线/曲线路径，并让宽度和透明度向尾端衰减即可。GPUI 0.2.2 的 `linear_gradient` 只有线性渐变，没有现成径向渐变；头部光晕可用 1–2 个透明圆角 quad 加 shadow 近似。

### 命中反馈

建议把命中拆成三个同步发生的视觉层：

1. **卡片脉冲**：在目标实时视觉矩形外绘制 1–2 个向外扩散、逐渐透明的圆角 outline；不要改变卡片真实 layout 或缩放正文内容。
2. **中心闪光**：目标中心绘制一个很短的高亮 quad/光晕，避免大面积白闪。
3. **粒子爆开**：12–18 个粒子，以稳定随机种子生成方向、速度和寿命；更新使用简单 Euler 积分、线性 drag 和可选轻微 gravity，大小/alpha 按剩余寿命衰减。

任务派发是“接受/路由成功”，不是任务完成。特效应在 receipt 接受后播放，不得用类似“完成烟花”的视觉语义误导用户：

- `Submitted`：正常亮度命中。
- `Queued`：可降低亮度或改成较柔和的环，表达已排队。
- `Untracked`：可以播放，但不应使用“受管成功”的专属颜色。
- `Failed`：不发射；走现有错误反馈。

批量派发时可按 35–50ms 轻微错开发射，但不能阻塞命令执行；同时活跃效果达到上限时应合并或降级。

## 性能判断

### 为什么小型 CPU 状态机足够

第一版最多约 20 个可见 primitive，运动学只涉及少量二维向量、插值和透明度计算。真正需要控制的是 GPUI 元素/layout 数量、每帧路径 tessellation 和 blur/shadow 数量，而不是粒子公式本身。

建议约束：

- 单个全窗口 `canvas`，不要“一粒子一个 GPUI element”。
- 同时活跃效果最多 3 个，总 impact 粒子最多 64 个。
- 每个飞行体只构建一条短尾迹 path；命中粒子优先用 `paint_quad`。
- 只给流星头和命中中心加 shadow，不给每个粒子加模糊阴影。
- 固定容量容器或复用缓冲，避免每帧无界分配。
- 无活跃效果时从 `MotionStore::activity_summary` 移除动画活动，让现有 frame driver 停止请求连续帧。
- 动画 `dt` 必须来自现有 frame clock，不要假设固定 60Hz；窗口卡顿后 clamp 大 `dt`，避免粒子瞬移穿过目标。

`PathBuilder::build()` 会通过 Lyon 在 CPU 端 tessellate，故“每粒子一条路径”不是好方案；一条 6–10 点的尾迹路径仍属合理规模，但最终必须测量，不能仅凭推断宣称无开销。

### 建议性能验收

以下是实现阶段的建议门槛，不是本次调研已测得的数据：

- 60Hz 帧预算为 16.67ms，120Hz 为 8.33ms。
- 在基线 Mac 上，特效更新 + 构建 + paint 的 p95 增量目标 `< 0.5ms/frame`。
- 单次派发、3 个并发派发、卡片拖动中派发、画布缩放中派发分别采样。
- 没有活跃特效后不得继续产生动画帧。
- 通过现有 `frame_diagnostics` 记录 path build 与 effect paint 时间；如果 p95 超标，先减少 shadow、路径点和粒子数，再考虑专用 Metal pipeline。

只有当未来需求扩展为长期存在、数百/数千粒子、纹理动画、bloom 或多个持续发射器时，才值得评估自定义 Metal/wgpu 粒子管线。当前不应提前承担这层复杂度。

## 与 Norma 现有代码的贴合点

- [`crates/app/src/motion/mod.rs`](../../crates/app/src/motion/mod.rs) 已有 `MotionStore::tick`、`activity_summary` 和 `reduce_motion`，可直接容纳短期特效状态。
- [`crates/app/src/app/frame_loop/animation_driver.rs`](../../crates/app/src/app/frame_loop/animation_driver.rs) 已用 `on_next_frame` 驱动活动动画，不需要另建 timer/thread。
- [`crates/app/src/cards/snake.rs`](../../crates/app/src/cards/snake.rs) 已在钉死的 GPUI 版本上验证 `canvas + PathBuilder + linear_gradient + paint_path`。
- [`crates/app/src/canvas/view.rs`](../../crates/app/src/canvas/view.rs) 已计算 `ResolvedNodeVisual.rect` 并转换为屏幕坐标，目标定位应复用同一语义。
- [`crates/app/src/app/chrome/dock.rs`](../../crates/app/src/app/chrome/dock.rs) 定义了 Dock 的居中、底部偏移和高度。
- [`crates/app/src/app/agent_runtime/commands.rs`](../../crates/app/src/app/agent_runtime/commands.rs) 已将 dispatch receipt 映射为 `Submitted / Queued / Untracked / Failed`，是触发语义的候选接缝。

## 实施阶段的验证清单

1. 纯状态机单测：阶段切换、帧率无关推进、目标移动、目标删除、并发上限、`reduce_motion`。
2. 具体正负断言：接受 receipt 必须创建效果；Failed receipt 不得创建；命中最后一帧必须落在最新目标中心；retire 后必须无活动帧。
3. e2e：从 Command Dock 触发到 Agent card 的可观察闭环，并覆盖排队/失败。
4. 页面/样式/视觉验收必须启动真实 App，用 Computer Use 检查正常速度、慢放/逐帧、拖卡与缩放时的命中准确度。
5. 深浅主题、不同缩放比例、窗口 resize 和 Reduce Motion 都要验收。

## 最终推荐

采用 **GPUI 原生单 Canvas + app 层轻量粒子状态机**。Rust 粒子项目用于借鉴 emitter/lifetime/velocity/curve 数据设计，不作为依赖。第一版把效果控制为一个 220–300ms 的制导流星和 140–180ms 的克制命中脉冲，确保它表达“任务被路由到这张卡”，而不是把任务派发误表现为任务完成。
