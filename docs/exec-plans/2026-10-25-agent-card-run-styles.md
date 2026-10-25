# Agent 卡片运行特效:经典 + 一道光 + 灵光 + 磁流体,及设置页

状态:R0–R8 已实现,未提交(见 §10)。视觉与交互以原型为准,本文只定义生产实现的分层、切片与验收:

- 特效本身:[`docs/prototypes/agent-card-run-effects/index.html`](../prototypes/agent-card-run-effects/index.html)(风格对比)
- 设置页:[`docs/prototypes/agent-card-run-effects/settings.html`](../prototypes/agent-card-run-effects/settings.html)
- 设计取舍:[`docs/design-docs/2026-10-25-agent-card-run-effects.md`](../design-docs/2026-10-25-agent-card-run-effects.md)

本期范围只有四种风格:**经典 Classic**(现状,默认)、**一道光 Underline**、**灵光 Aura**、**磁流体 Ferrofluid**。设计文档里的其余风格(一个点、一条线、等高线、声振、扫描)不在本期范围。本文取代设计文档 §4 的 T0–T9。

## 1. 目标与非目标

目标:
- 用户在 设置 → 通用 → 特效 的「卡片运行特效」分区选择一种风格,所有项目共用,选择后立即作用于画布上所有 Agent 卡片。
- 四种风格覆盖六个状态:准备、思考、工具调用、等待审批、完成(未查看)、失败。
- 默认仍为经典,升级后用户看到的效果不变。

非目标:
- 按工作空间或项目分别设置。
- 每张卡片单独设置。
- 自定义颜色。
- 手机端设置。
- 统计事件。

## 2. 现状核对(已读代码)

| 需要 | 已有 | 用法 |
| --- | --- | --- |
| 状态来源 | `AgentTaskState`(`Ready / Thinking / RunningTool{tool_name} / WaitingApproval{tool_name} / Done / Failed{message} / Ended`),卡片上还有 `CardAttentionState{kind: AgentTaskDone, viewed}` | 直接映射为运行状态,见 §4 |
| 经典:思考与工具调用 | `agent_chase_intent` → `WorkingSnakeView`(`cards/snake/view.rs`),它是独立的 Entity,有自己的显示帧驱动,每帧只 notify 自己;周期 1.2s,内缩 3px,线宽 3px | 原样保留;新风格的宿主照这个模式做 |
| 经典:等待审批 | `agent_breathing_intent` → `BreathingIntent::Warm`,暖色 1.2s 呼吸边框(`motion/breathing.rs`) | 原样保留 |
| 经典:完成未查看 | `BreathingIntent::Cool`(冷色 2.5s 色相循环)+ `BorderGlow` 外缘三层柔光(`cards/border_glow.rs`)+ `completion_flash`;从运行中进入完成时还有 `completion_settle` 绿色收尾 | 原样保留 |
| 经典:失败 | 无边框特效,只有状态胶囊变成错误色 | 原样保留;原型里的经典已按此修正 |
| 合成位置 | `canvas/view.rs` 中 `render_border_glow` 与 `working_snakes::synchronize_view` | 新风格在同一位置挂载宿主 |
| 标题栏 | `cards/chrome.rs` 中 `TITLE_BAR_H = 32.0` | 一道光画在标题栏下边线上 |
| 外缘留白 | `border_glow.rs` 中 `MARGIN_PX = 12.0`,向卡片外扩一圈画布 | 灵光、磁流体同样外扩,留白更大 |
| 设置页 | `settings/effects/{view,state,page,current,grid,preview,playback,…}.rs`;动作在 `app/effects_actions.rs`(457 行,已接近 500 行上限);偏好存在 `SettingsStore` 的 `effects.*` 键;撤销通过保存状态行实现 | 新分区与新动作都放新文件 |
| 减少动态效果 | `motion.reduce_motion`(环境变量)+ `motion::system_reduce_motion()`(系统设置) | 新风格同样遵循 |
| 绘制原语 | `PathBuilder` 加 `paint_path(path, Background)`,支持 `linear_gradient`(蛇用了);`paint_quad` 支持 `box_shadow`;没有加法混合,也没有路径模糊 | 辉光用多层半透明描边近似 |

## 3. 分层

```text
storage        只用现有 SettingsStore 存字符串键 effects.run_style,不新增迁移
app/motion     run_style/:RunStyle 枚举、RunState 映射、RunMotion 平滑与节流、各风格的纯函数 → Vec<RunPrim>
app/cards      run_effect/:RunEffectView(每张卡片一个 Entity,自带帧驱动)+ paint.rs(RunPrim → GPUI)
app/canvas     run_effects.rs:挂载与同步(同 working_snakes.rs)
app/settings   effects/run_*.rs:设置页分区;app/run_effects_actions.rs:加载、保存、撤销
```

- 各风格的纯函数输入为 `RunFrameInput { state, since_change, since_ping, smoothed, rect, radius, zoom, viewed, seed }`,输出图元列表,不含 GPUI 类型,可单测。
- 图元 `RunPrim` 包括:`Polyline { pts, width, color }`、`FilledPath { pts, holes, fill: Solid | VerticalGradient }`、`Disc`、`LineSeg`。
- 风格选择在渲染路径上只读内存缓存(gpui `Global`),不读 SQLite。
- 经典不进 `run_style` 的绘制函数,继续走现有的蛇、呼吸和柔光代码。只有当前风格不是经典时,才关掉这三者,改挂 `RunEffectView`。

## 4. 状态映射与共同规则

| `AgentTaskState` + 注意力 | `RunState` |
| --- | --- |
| `Ready` / `Ended` / 无 | `Ready` |
| `Thinking` | `Thinking` |
| `RunningTool{tool_name}` | `Tool`;`tool_name` 变化时触发一次 ping |
| `WaitingApproval` | `Approval` |
| `Done` + 未查看 | `Done{viewed:false}` |
| `Done` + 已查看或无注意力 | `Done{viewed:true}`,特效淡出 |
| `Failed` | `Failed`,保留到下一次任务开始 |

- **可中断**:强度、颜色、各分量权重都用指数平滑(时间常数 140–450ms,按原型 `sm()` 的参数),新状态从当前值出发,不重置。
- **ping 节流**:同一张卡片 600ms 内最多一次。
- **停帧**:`Ready` 状态且所有平滑值都小于 ε 时停止帧驱动;卡片不在视口内、或缩放后小于 120px 宽时,宿主不请求帧。
- **减少动态效果**:一道光只保留颜色(整条线显示状态色,无游走、无心跳);灵光和磁流体统一回落为卡片底部 3px 状态色细条,160ms 淡变。
- **深浅主题**:状态色取自 `Theme`(`accent_blue`、`accent_cyan`、`accent_warm`、`accent_green`、`accent_red` 等),不在风格里写死 RGB。

## 5. 各风格落地要点

| 风格 | 画在哪 | 图元规模(单卡每帧) | GPUI 要点 |
| --- | --- | --- | --- |
| 一道光 | 标题栏下边线 `y = TITLE_BAR_H·zoom`,左右各内缩 1px | 1–2 条线段;思考与工具调用时是 70px 宽的光段 | 光段两端渐隐用横向 `linear_gradient`,不画逐段 alpha;完成时从左向右铺满(600ms ease-out) |
| 灵光 | 卡片边缘外侧 16px 内,并向卡片内渗光 | 周长切成 24–36 段,每段 3 层描边(宽约 2/7/14px,alpha 递减),加内侧一层 | 没有模糊:用宽描边 + 低 alpha 叠层近似原型的离屏放大;每段一个颜色(光谱调色板随时间旋转);工具 ping 时一束亮光沿周长疾行;失败时闪 2–3 下后收成暗红细环 |
| 磁流体 | 卡片外缘一层膜,外扩 40px(液滴、滴落最长 34px) | 外轮廓 240 点的闭合路径 + 卡片圆角矩形作洞,一次填充;再加 1 条描边和 1–2 段高光 | 环形填充依赖 `PathBuilder` 的填充规则,R0 先验证;本体用竖向渐变填充;尖刺、液滴、滴落都是确定性的 hash,按 `seed + 时间片` 计算 |

预算:单卡每帧 build + paint ≤ 0.4ms(Release,M 系列芯片)。同屏有 6 张以上运行中的卡片时,灵光分段 36 → 24,磁流体采样点 240 → 160。用 `frame_diagnostics` 新增的 `RunEffectPaintReport` 验收,做法同 `SnakePaintReport`。

## 6. 切片与顺序

每片都先写失败测试再实现,可以单独合并;单文件 ≤ 500 行,不用 `#[allow]`。

顺序:R0 → R1 → R2 → R3 → R4 →(R5、R6 可并行)→ R7 → R8。

**R0 技术探针(半天以内,结论写回 §9)**
- `PathBuilder` 带洞的闭合路径能否一次填出环形(磁流体)。不行就把膜切成 4 段四边形带分别填充。
- 灵光 36 段 × 3 层描边在 Release 下的耗时;以及在深色主题下与原型的观感对比截图。
- `canvas()` 元素外扩 40px 后,是否被父容器的 `overflow_hidden` 裁掉(参考 `border_glow.rs` 的挂载方式)。

**R1 偏好与缓存**
- 新增 `motion/run_style/mod.rs`:`RunStyle { Classic, Underline, Aura, Ferrofluid }`,提供 `id()` / `from_id()`;未知值或空值回落到 `Classic`。
- 新增 `app/run_effects_actions.rs`:启动时在 `load_effect_settings` 旁读取 `effects.run_style`,写入 `EffectsSettingsState.run_style`;保存放在后台线程,界面先乐观更新,保存失败时回滚。
- 测试:id 往返;未知值回落;保存失败回滚;撤销恢复上一个值。

**R2 运行状态与平滑(纯函数)**
- 新增 `motion/run_style/state.rs`:实现 `RunState::from_task(state, attention)`,以及 `RunMotion`(平滑值、`since_change`、`since_ping`、ping 节流、停帧判断)。
- 测试(`crates/app/tests/unit/run_style_state_tests.rs`):
  - 上面的映射表;
  - 状态在中途切换时颜色和强度从当前值出发;
  - 600ms 节流;
  - 同名工具不触发 ping;
  - 已查看后淡出到 0 并停帧;
  - 失败保留到下一次 `Thinking`;
  - `dt` 很大时数值不发散;
  - 减少动态效果时没有周期分量。

**R3 宿主与接入(此时还没有任何新风格)**
- 新增 `cards/run_effect/{view.rs,paint.rs}`:`RunEffectView` 照搬 `WorkingSnakeView` 的帧驱动与 `visible` 处理,`paint.rs` 负责把图元转成 GPUI 调用。
- 新增 `canvas/run_effects.rs`:做挂载、同步与回收。
- 在 `canvas/view.rs` 按风格分流:
  - `Classic`:代码路径与现在完全一致;
  - 其他风格:对 Agent 卡片关闭蛇、呼吸、柔光,改为挂载宿主。
- 测试:
  - 经典风格下 `ResolvedNodeVisual` 快照与现状一致(防回归);
  - 非经典风格下 `working_snake`、`border_glow` 为 `None`,且呼吸关闭;
  - 视口外的卡片不请求帧。
- 跑 `check-performance-guardrails.sh`。

**R4 一道光**(最简单,用来打通整条管线)
- 新增 `motion/run_style/underline.rs`。
- 单测:
  - 光段位置在 2.4s 内来回;
  - 工具调用时光段平滑到工具名下方;
  - 完成时铺满的进度单调;
  - 减少动态效果下是整条线、alpha 恒定;
  - 缩放时线宽乘以 zoom。

**R5 灵光**
- 新增 `motion/run_style/aura.rs`。
- 单测:
  - 调色板在状态切换时平滑过渡;
  - 工具 ping 时的亮点沿周长单调前进;
  - 失败时的闪烁序列由种子决定、结果可复现;
  - 分段数与图元数不超过上限;
  - 大量卡片时自动降级。

**R6 磁流体**
- 新增 `motion/run_style/ferrofluid.rs`。
- 单测:
  - 膜的偏移不为负;
  - 等待审批时液滴位于底边中点;
  - 滴落长度不超过 34px;
  - 外扩不超出留白;
  - 同一种子输出一致;
  - 采样点数不超过上限。

**R7 设置页「卡片运行特效」分区**
- 新增 `settings/effects/{run_section.rs, run_preview.rs, run_tiles.rs}`,在 `view.rs` 中放在 `playback_section` 之后。
- 分区标题注明「所有项目共用」,不受页面顶部工作空间/项目范围切换影响。
- **当前大预览**:
  - 六个状态自动轮播,时长取原型的 `DUR`;
  - 状态芯片可以固定某个状态,「自动」恢复轮播;
  - 点击预览卡片等于标记已查看。
- **风格网格**:
  - 四格,复用 `WallpaperLayout` 计算列数;
  - 静止时显示定格帧,悬停或键盘聚焦时才播放;
  - 点击、Enter、Space 立即生效;
  - 撤销复用保存状态行上的「撤销」按钮。
- 预览复用 `motion/run_style` 的纯函数和 `cards/run_effect/paint.rs`;经典的预览复用 `build_snake_paths`、`breathing_color_for_phase` 和 `glow_bands`,不另写一套。
- 只在设置窗口可见、且该分区处于激活状态时才请求帧;减少动态效果时网格不播放,大预览只随状态变色。
- 可交互元素都要有 hover、pressed、键盘焦点、选中状态,遵循 `docs/quality/ui-interaction-feedback.md`。
- i18n:在 `settings.zh-CN.json` / `settings.en.json` 中新增 `effects.run_*` 键。
- 测试:选择、撤销、保存失败回滚;芯片固定与恢复自动的状态机;布局列数与壁纸页一致;i18n 键齐全。

**R8 真实 App 验收**
- `NORMA_VISUAL_PREVIEW` 增加 `run-style-<id>-<state>` 定格值,风格 × 状态 = 4 × 6。
- 新增 desktop-e2e 场景:
  - settings 模块新增 `settings-run-effects`:选择风格、撤销、深浅主题、重启后设置保留;
  - canvas 模块新增 `canvas-run-effects`:逐风格定格截图,并断言「已查看后无残留帧」。
  - 看图清单放在 `checks/<模块>/`。
- 用 `review-animations` 审查四种风格;按 `agent-execution-policy.md` 起独立 subagent 做对抗式代码审查。

## 7. 验收标准与证据

| 标准 | 证据 |
| --- | --- |
| 默认经典,升级后视觉与现在一致 | R3 快照测试 + 现有 `motion`/`cards` 测试全部通过 + 真实 App 对比截图 |
| 选择后画布上所有 Agent 卡片立即切换,无需重启 | R1 测试 + `settings-run-effects` |
| 重启后选择保留 | `settings-run-effects` |
| 三种新风格的六个状态与原型一致,深浅主题都能看清 | `canvas-run-effects` 定格截图,逐张看图 |
| 状态切换无跳变;已查看后淡出;失败保留到下一次任务 | R2 单测 + 真实 App 连续状态录屏抽帧 |
| 减少动态效果下没有位移和节奏 | R2/R4 单测 + `MULTISOUL_REDUCE_MOTION=1` 真实 App |
| 单卡 ≤ 0.4ms;视口外与 Ready 状态不出帧 | `RunEffectPaintReport`(Release)+ R3 单测 + 性能护栏脚本 |
| 设置页交互状态齐全,键盘可完成全部操作 | `settings-run-effects`(浅色/深色) |

每片合并前运行:
- `cargo test -p app`(受影响的测试);
- `bash scripts/check-max-file-lines.sh`;
- `bash scripts/check-no-allow.sh`;
- `bash scripts/check-performance-guardrails.sh`。

## 8. 设计记录

参考的 skill:`apple-design`(内容优先、即时反馈、可访问性)、`emil-design-eng`(动效服务于状态理解、可中断、高频场景克制)、`docs/design-docs/material-menus.md`(网格与选择器)、`review-animations`(R4–R6 合并前)。

采纳:
- 设置页与派发特效同构(当前大预览 → 网格),用户不需要学新布局;
- 设置为全局,因为运行特效是阅读画布的语言,不同项目间不应变化;
- 默认保持经典,新风格靠用户主动选择;
- 每张卡片一个自带帧驱动的 Entity,动画帧不重绘整个画布。

拒绝:
- 加法混合与模糊(GPUI 0.2.2 不支持,改用叠层);
- 在 paint 阶段保存粒子状态(改用种子和 hash,结果可复现);
- 用新风格重写经典(风险高,且没有收益)。

## 9. 待确认与风险

- **「完成提示保留到查看」开关**(原型里有,原方案没有):建议本期不做,保持现状(总是保留到查看),R7 少一个设置项。如果要做,新增键 `effects.run_keep_done`,在 R2 增加自动标记已查看的计时。
- **「遵循系统减少动态效果」开关**:派发特效已按产品要求删除了同类开关。建议运行特效总是遵循系统设置,不提供开关;原型里的这一行在 R7 中不实现。
- **深色主题观感**:灵光没有加法混合,亮度会低于原型;R0 截图后再决定是否提高叠层 alpha。
- **磁流体成本与留白**:外扩 40px 可能盖住相邻卡片或连线;R0 验证层级,必要时把液滴与滴落上限降到 24px。
- **键名**:设计文档原写 `agent_card.run_style`,本计划改为 `effects.run_style`,与现有 `effects.*` 键保持一致;合并 R1 时同步修改设计文档。

## 10. 实施记录(2026-10-25)

按 §9 的建议执行:不做“完成提示保留到查看”开关(总是保留到查看);运行特效总是遵循系统减少动态效果,不提供开关。

**R0 结论**
- `PathBuilder::fill()` 默认 `FillOptions` 为奇偶规则,外轮廓 + 卡片轮廓一次画出磁流体环形膜,无需切块。
- 灵光原定 36 段 × 3 层;实测改为 24 段 × 4 条色带(相邻段共享顶点,转角无缝),单卡绘制均值 0.39ms。
- 布局间距只有 8 屏幕像素(`layout_strategy.rs`),外缘特效画在卡片之上会盖住邻卡。改为:灵光、磁流体画在**所有卡片之下、壁纸之上**(`render.rs` 把壁纸从缓存的画布层挪到根节点,特效层夹在中间);一道光在卡片内部,画在卡片之上。

**落地文件**
- 纯函数:`crates/app/src/motion/run_style/`(`mod`、`geometry`、`palette`、`state`、`underline`、`aura`、`ferrofluid`)。
- 绘制宿主:`crates/app/src/cards/run_effect/`(`RunEffectView` 30fps 自带帧驱动,定格或不可见时停帧);挂载 `crates/app/src/canvas/run_effects.rs`。
- 设置:`crates/app/src/settings/effects/run_section.rs`、`run_preview.rs`;动作 `crates/app/src/app/run_effects_actions.rs`(键 `effects.run_style`)。
- e2e 夹具:控制通道新增 `run_effects_fixture`(`crates/app/src/app/run_effects_fixture.rs`,仅 debug 构建),桌面 e2e 的 App 不带 `e2e-test-support`,不能用 `NORMA_VISUAL_PREVIEW`。

**实现中的决定**
- 完成(未查看)与失败在 1.6s 后、且平滑值收敛后定格停帧,不常驻 60fps;等待审批、思考、工具调用持续出帧。
- 设置页预览不存逐帧状态:每次渲染按时间轴从“准备”回放到当前时刻,定格规则与画布一致。
- 保存按序号串行:较旧的写入晚到时跳过;只有最新一次失败才回滚到最后确认写入的值;启动读取晚于用户点选时不覆盖。

**验证**
- 单测:`run_style_tests`(13)、`run_style_render_tests`、`run_preview_tests`(5)、`canvas_run_effects_tests`(经典意图不变)。全量 `cargo test -p app -p ui` 连跑两次均无失败(app 1514 通过)。此前的两个失败已修:`command_dock_pending_preview_does_not_block_new_commands` 仍假设 `/preview` 在指令候选里(9524d38a 已移除),改为断言等待中再回车不建卡、不替换等待中的预览;`help_diagnostics_explain_blockers_without_logging_user_text` 是 tracing callsite 兴趣缓存在并行测试中被其他线程抢先缓存为“不关心”,测试里装好订阅者后重建一次兴趣缓存。
- 机械约束:`check-max-file-lines`、`check-no-allow`、`check-performance-guardrails`、`check-agent-guide-lines`、`check-video-wallpaper-performance` 全部通过。
- 真实 App:`canvas-run-effects` 4 风格 × 浅/深 9/9 步通过,证据 `artifacts/desktop-workspace-e2e/2026-10-25T09-57-26Z/`;`settings-run-effects` 7/7 步通过,证据 `artifacts/desktop-workspace-e2e/2026-10-25T10-27-55Z/`。
- 绘制耗时(`MULTISOUL_FRAME_DIAGNOSTICS=1`,debug 优化构建,单卡 build+paint 均值):灵光 0.39ms(96 条路径)、磁流体 0.25ms(≤20 图元)、一道光 ≤2 图元。
- 对抗式审查(独立 subagent)提出的 9 条中已修 7 条:外缘特效遮挡邻卡、保存竞态、减少动态效果切换停在中间值、保存失败仍显示成功图标、测试文件超 500 行、e2e 断言假阳性(改为只测特效区、磁流体用“偏绿像素数”)、预览回放上限(24s 循环)。

**已知局限**
- 设置页预览用 `request_animation_frame`,播放时会让整个窗口每帧重绘;与现有派发特效预览同一机制,未拆成独立 Entity。
- 画布可见性裁剪只按卡片本体,刚移出视口的卡片外缘特效会提前消失。
- 同屏多卡时未做动态降级(实测单卡已在预算内,6 张约 2.3ms/帧)。
- 键盘操作(Tab 到格子、Enter 选中)有焦点态与键盘处理,但 e2e 没覆盖;保存失败回滚没有自动化测试。
