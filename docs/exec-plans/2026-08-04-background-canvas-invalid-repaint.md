# 后台 Canvas 持续高 CPU / 无效重绘：诊断与分阶段实施计划

> 状态：Proposed / Not Implemented  
> 日期：2026-08-04  
> 当前决策：先实施 P0，仅处理后台 Canvas 的无效重绘；P1/P2 必须基于 P0 后的真实数据再决定。  
> 相关计划：[`2026-07-25-cpu-p0-p1-scheduling-and-terminal-cache.md`](2026-07-25-cpu-p0-p1-scheduling-and-terminal-cache.md)、[`2026-07-25-cpu-p2-p3-local-invalidation-and-incremental-rendering.md`](2026-07-25-cpu-p2-p3-local-invalidation-and-incremental-rendering.md)

## 1. 一句话方案

使用“事件驱动重绘 + 窗口状态门控”：后台暂停 Canvas 的视频和装饰动画，只有 Terminal、Agent、Runtime 等真实业务状态发生变化时才允许重绘；业务任务始终继续运行，回到前台后恢复动画并显示最新状态。

## 2. 背景与目标

近期性能日志显示，Norma OS 在窗口进入后台后仍长期保持较高的 CPU、wakeups 和 repaint/render 频率。问题的核心不是一次渲染过慢，而是没有可见业务变化时，窗口仍被动态媒体或动画路径持续唤醒。

本计划目标：

- P0 先切断后台 Canvas 的装饰性无效重绘，控制改动范围。
- 保证 Terminal、Agent、Runtime、语音和持久化等业务执行不暂停、不丢状态。
- P0 完成后重新采样；只有剩余问题有明确证据时才进入 P1/P2。
- 不以简单降到 10/15/30 FPS 作为最终解法；静止态目标是事件驱动和接近零重绘。

## 3. 已观察到的问题

| 问题 | 当前状态 | 证据与判断 | 优先级 |
|---|---|---|---|
| 后台持续高 CPU | 仍存在 | 真实进程曾观察到约 23%～33% CPU；内部后台报告同时出现约 35 repaint/s | P0 |
| 后台 repaint 与 render 基本一一对应 | 仍存在 | 多组报告中 repaint/s 与 render/s 都约为 32～35，说明不是轻量空 tick，而是实际进入绘制 | P0 |
| Canvas 视频帧不感知窗口失焦 | 已确认 | 视频线程和 40ms poll loop 继续推进；frame changed 后可直接 `cx.notify()` | P0 |
| 根动画循环存在后台视觉推进路径 | 已确认 | 后台若仍有 pending animation frame，会继续 `advance_animation_frame`，变化后通知重绘 | P0 |
| GIF 可绕过 Norma 的重绘门控 | 已确认的结构性风险 | GPUI 0.2.2 的多帧 `img()` 会自行 `request_animation_frame()` | P1；P0 只做窄路径验证 |
| Launchpad/License 固定视频不受 Canvas 壁纸设置影响 | 已确认 | 即使 Canvas 当前选择静态壁纸，其他窗口/页面仍可能独立播放固定视频 | P1 |
| 内部 CPU 指标与系统采样不一致 | 仍存在 | 本机内部值与 `top`/`ps` 对照约低估 41.67 倍；正式阈值前需要修正/复核 Mach timebase | P1 验收配套 |
| 缺少完整 repaint 来源归因 | 仍存在 | 现有 diagnostics 能覆盖 Runtime/motion，但 GPUI 子元素或媒体自驱动不一定进入同一原因集合 | P2 |

## 4. 关键日志证据

2026-08-04 日志中的代表性后台汇总：

| 时间（UTC） | 状态 | 样本数 | 内部 CPU 中位数 | wakeups/s | repaint/s | render/s |
|---|---:|---:|---:|---:|---:|---:|
| 00:27:02 | background | 1800 | 0.8279 | 579.86 | 35.17 | 35.17 |
| 01:27:14 | background | 1800 | 0.6947 | 726.00 | 35.20 | 35.09 |
| 01:57:19 | background | 1800 | 0.4191 | 789.76 | 17.75 | 17.65 |

额外观察：

- 安装版进程曾在启动后短时间内达到约 32.7% CPU、约 817 MB RSS。
- 一次 5 秒系统采样显示主线程反复进入 Metal/`CAMetalLayer nextDrawable` 和绘制调用，说明 CPU 消耗与真实渲染循环相关。
- 2026-08-01 的 frame diagnostics 中，约 49,598 次 `runtime_frame_tick` 均显示 `runtime_changed=false`、`agent_hook_changed=false`、`agent_hook_pending=false`、`should_notify=false`。
- 同一批 animation diagnostics 绝大部分为 `notify_reason=none`，工作蛇、呼吸、布局也没有持续 active。

由此可以排除“Runtime 每 tick 都 dirty”作为该批无效重绘的主要解释；需要优先处理绕过或独立于 Runtime repaint gate 的视觉驱动。

> 注意：当前持久化 Canvas 壁纸为单帧 `wallpapers/1411385.webp`，已确认不是多帧 WebP。因此 GIF 是确定的结构性风险，但不是该静态素材本身的直接驱动。Launchpad、License 或其他窗口的视频生命周期需单独判断。

## 5. 技术根因链

### 5.1 视频壁纸路径

当前视频路径为：

```text
后台 decoder 按 PTS 继续取帧
  → 发布到 VideoFrameMailbox
  → GPUI 侧每 40ms poll
  → 发现 changed
  → cx.notify()
  → 整个窗口重新 render
```

`VideoWallpaperState` 已经具备 `paused` 和 `set_paused()`，但该能力主要被 Feature Guide 手动暂停使用，没有形成统一的窗口失焦策略。这意味着 P0 不需要重写解码器，先复用现有 pause seam 即可。

### 5.2 根动画路径

`AnimationSchedule` 已能把非活动 Canvas 退回 timer，但仍存在 pending frame driver 在后台推进视觉状态并在 changed 后通知的分支。P0 应明确：后台 Canvas 的纯视觉变化不拥有重绘权。

### 5.3 GIF 路径

GPUI 0.2.2 在带稳定 element id 的多帧 `img()` 中会调用 `window.request_animation_frame()`；该请求由 GPUI 自己安排下一帧，无法被 Norma 的 Runtime repaint gate 直接合并。

P0 不应因此立即扩展为 GIF 解码器重构。实施时先验证一个窄方案：后台 Canvas 是否可以把 GIF 壁纸切为无动画的单帧呈现且不触发下一帧；如果该方案不能通过自动化证据，GIF 留到 P1，不扩大 P0。

## 6. 业务边界

### 必须继续运行

- `Runtime::drain_all` 和输入路由。
- PTY 读取、Terminal grid 更新、退出状态和 resize。
- Agent 任务、队列、hook、完成事件和会话状态。
- ASR 识别、TTS 音频播放和必要的音频生命周期。
- 浏览器/Ego 等后台业务事件接收。
- 持久化和必要 checkpoint。

### 允许后台暂停或冻结

- Canvas 视频/GIF 壁纸。
- Terminal 光标视觉相位。
- Voice 波形、窗口内缘等纯视觉反馈；音频不能暂停。
- Working Snake、呼吸光和 dispatch 粒子等装饰动画；Agent 任务本身不能暂停。

### P0 的产品行为

当 Canvas 窗口失焦时，动态壁纸冻结在最后一帧或安全的静态帧；回到前台后恢复。放在第二显示器但不是 key window 的 Canvas 也会采用该策略。

P0 不新增用户设置。后台阻止装饰性空转是默认能源策略；如果真实用户反馈需要“非激活但仍可见时继续动画”，再在 P2 讨论更细的 visibility/occlusion 策略。

## 7. 分阶段优先级

### P0：后台 Canvas 无效重绘止血

#### 改动点

1. 增加一个纯策略判断，仅区分：
   - 真实业务变化：允许通过现有预算合并重绘。
   - 纯视觉 tick/媒体帧：后台 Canvas 不允许触发重绘。
2. 使用窗口 activation 事件及时更新 `window_active`；不只依赖下一次 `render()` 被动刷新状态。
3. 当 `screen == Canvas && window_active == false`：
   - 对 Canvas 视频调用现有 `set_paused(true)`。
   - 视频 poll 即使取到 changed，也不得 `cx.notify()`。
   - 根动画 pending 分支不再推进并通知纯视觉帧。
4. 恢复前台时：
   - `set_paused(false)`。
   - 只触发一次恢复重绘。
   - Runtime 当前状态和最新媒体帧一次性呈现。
5. GIF 仅做窄验证：能以很小改动冻结单帧则纳入；否则显式留到 P1。

#### 建议修改位置

- `crates/app/src/app/frame_schedule.rs`：纯 Canvas 后台通知策略。
- `crates/app/src/app/frame_loop.rs`：后台 pending animation 分支。
- `crates/app/src/app/video_wallpaper.rs`：Canvas pause/resume 与 notify gate。
- `crates/app/src/app/render.rs` 或 bootstrap/window subscription：可靠的 activation 状态更新。
- `crates/app/tests/unit/`、`crates/e2e/`：聚焦证据。

#### 明确不做

- 不重构共享媒体播放器。
- 不修改 Launchpad、License、Feature Guide。
- 不实现新的 GIF 解码管线。
- 不拆 Node Entity，不做局部 Canvas 重构。
- 不暂停 Runtime/PTY/Agent。
- 不增加性能设置项。

#### 影响面

仅影响失焦 Canvas 的装饰视觉。前台 Canvas、业务执行和其他页面不变。

#### 预期收益

在“单 Canvas 窗口、后台、无业务变化”的固定场景下：

| 指标 | 当前代表值 | P0 目标 |
|---|---:|---:|
| repaint/s | 32～35 | 稳态 ≤0.1 |
| render/s | 32～35 | 稳态 ≤0.1 |
| Norma OS CPU 中位数 | 系统采样约 23%～33% | `<5%`，优先争取 `<2%` |
| 业务状态完整性 | 基线 | 不得回归 |

如果后台 Terminal/Agent 持续产生真实变化，允许按现有 background budget 合并重绘；该情况不属于“无效重绘”。如果同一进程还有 Launchpad/License 视频窗口，进程总 CPU 可能仍高，P0 必须按单 Canvas 场景验收。

#### P0 退出条件

- 单 Canvas 后台静止 5 分钟，repaint/render 稳态接近零。
- PTY 在后台持续写入，Runtime 持续 drain，恢复前台后最终 grid 完整。
- Agent 状态在后台继续推进，恢复前台后显示最新状态。
- 视频壁纸前后台切换 50 次，无黑屏、旧 decoder 通知或崩溃。
- 若 P0 后 CPU 仍高，必须重新 sample；不得直接进入大重构。

### P1：动态媒体生命周期收口

#### 启动条件

- P0 后 Launchpad/License/Feature Guide 仍能在后台保持明显 CPU；或
- Canvas GIF 仍绕过后台 gate；或
- 视频 decoder/poll wakeups 成为剩余主要热点。

#### 改动点

- 把视频的固定 40ms poll 改为 frame-ready 事件唤醒。
- 将 pause/resume 扩展到 Launchpad、License、Feature Guide。
- 为旧 decoder/poll 增加 generation 失效保护。
- GIF 改为可控生命周期：后台静态帧、前台继续播放。
- 修正并复核内部 CPU timebase，使性能阈值可信。

#### 影响面

所有动态背景和教程媒体；不触碰业务 Runtime。Feature Guide 必须保留播放位置，GIF 的帧延迟、透明度和循环语义需要兼容验证。

#### 预期收益

- 所有后台页面接近零媒体重绘。
- 消除 40ms 空轮询。
- 后台 CPU 中位数稳定 `<2%`。
- wakeups 相对当前 580～790/s 显著下降。

### P2：统一视觉活动调度与防回归

#### 启动条件

- P1 后仍有无法归因的高频 repaint；或
- 新增动画继续出现各自独立的 frame driver；或
- 多窗口/多显示器策略需要产品化。

#### 改动点

- 统一管理 Video/GIF、Working Snake、Cursor、Voice、Dispatch Effect 的活动状态和下一视觉边界。
- 区分 key window、非激活但可见、最小化和完全遮挡。
- 多窗口独立管理媒体生命周期。
- 增加按来源聚合的 repaint attribution。
- 扩展 L3 性能场景和静态 guardrail，禁止动态壁纸回退到不受控的 `img()`/逐帧 CPU 图像路径。

#### 影响面

涉及全 App 视觉基础设施，风险和验证成本最高。只有 P0/P1 后证据仍支持时才实施。

#### 预期收益

- 每一次重绘都有明确来源。
- 无视觉活动时完全事件驱动。
- 新动画功能不容易再次引入后台高 CPU。

## 8. P0 验收标准与自动化证据

| 验收标准 | L1/L2 自动化 | L3 真实 App |
|---|---|---|
| 后台 Canvas 纯视觉变化不得 notify | 纯策略和 frame loop 单测 | 单 Canvas 后台 5 分钟 repaint 统计 |
| 视频后台暂停、前台恢复 | Video state/poll 单测 | 视频 Canvas 前后台切换 |
| 单次业务 dirty 不丢失 | RepaintGate/Runtime 单测 | 后台 PTY burst，恢复后检查最终内容 |
| Agent 任务不因失焦暂停 | e2e 状态推进 | 真实 Agent 后台运行再恢复 |
| 前台动画不受影响 | animation schedule 既有回归 | Computer Use 检查视频、光标、工作蛇、Voice |
| 多次切换无旧任务通知 | generation/pause 状态测试 | 前后台切换 50 次并观察 CPU/日志 |

真实 App 验收必须使用 `scripts/run-cpu-performance-l3.sh` 的 `background-seven-cards` 或新增的单 Canvas 动态壁纸场景。Headless GPUI 只能验证状态与通知契约，不能替代真实 `.app` CPU/repaint 证据。

所有新增 Rust 测试必须遵守项目测试规范：完整写明场景、数据构造与数值推导、执行过程、预期结果；每个断言带失败说明，并逐项包含正、负验收标准。

## 9. 实施顺序

1. 冻结单 Canvas 后台基线：CPU、repaint、render、wakeups、窗口和壁纸类型。
2. 先写纯策略失败测试，证明后台视觉帧当前会被允许。
3. 最小实现 Canvas background gate。
4. 接入视频 pause/resume；保持其他页面不变。
5. 补 Runtime/PTY/Agent 不丢状态的 e2e。
6. 构建真实 `.app`，运行 L3 对比。
7. 达到 P0 退出条件后停止；根据剩余 profile 决定是否开启 P1。

## 10. 风险与回退

| 风险 | 预防/回退 |
|---|---|
| 后台 PTY 不再 drain，channel 积压 | P0 只 gate notify，不 gate Runtime；e2e 明确验证最终输出 |
| 恢复前台后画面仍停在旧帧 | activation 恢复时显式 notify 一次 |
| 视频 decoder 已暂停但 poll 仍频繁唤醒 | P0 记录 wakeups；若成为主要剩余成本，P1 改事件驱动 |
| 第二显示器上的非 key Canvas 动画冻结 | 这是 P0 明确产品行为；若不可接受，P2 再引入可见性策略 |
| GIF 仍自行申请下一帧 | P0 不隐瞒残留；窄方案不成立则留到 P1 |
| 进程总 CPU 未下降 | 先确认是否还有 Launchpad/License/其他窗口；按窗口归因，避免误判 P0 |
| 内部 CPU 指标继续低估 | P0 使用系统采样作为主要门禁；P1 修正内部 timebase |

## 11. 代码与证据导航

- Root render 和 `window_active`：[`crates/app/src/app/render.rs`](../../crates/app/src/app/render.rs)
- Runtime/animation/persistence frame loops：[`crates/app/src/app/frame_loop.rs`](../../crates/app/src/app/frame_loop.rs)
- 动画与 repaint 策略：[`crates/app/src/app/frame_schedule.rs`](../../crates/app/src/app/frame_schedule.rs)
- 视频 mailbox、poll、pause：[`crates/app/src/app/video_wallpaper.rs`](../../crates/app/src/app/video_wallpaper.rs)
- Canvas 壁纸/GIF `img()`：[`crates/app/src/app/chrome.rs`](../../crates/app/src/app/chrome.rs)
- Launchpad 固定视频：[`crates/app/src/workspace/launchpad/mod.rs`](../../crates/app/src/workspace/launchpad/mod.rs)
- License 固定视频：[`crates/app/src/license_gate/gate_root.rs`](../../crates/app/src/license_gate/gate_root.rs)
- Performance monitor：[`crates/app/src/performance_monitor.rs`](../../crates/app/src/performance_monitor.rs)
- macOS process sampler：[`crates/app/src/performance_monitor/macos.rs`](../../crates/app/src/performance_monitor/macos.rs)
- L3 runner：[`scripts/run-cpu-performance-l3.sh`](../../scripts/run-cpu-performance-l3.sh)
- 性能场景：[`docs/references/cpu-performance-scenarios.md`](../references/cpu-performance-scenarios.md)

## 12. 当前结论

P0 的最佳性价比不是建立新的全局动画框架，而是在现有 Canvas 高频通知入口增加窗口状态门控，并复用已有视频 pause seam。它只改变后台装饰视觉，不改变业务执行；达到目标后立即停止，通过数据决定是否继续 P1/P2。

本文档仅记录诊断和实施计划。截至 2026-08-04，尚未修改任何生产代码或测试代码。
