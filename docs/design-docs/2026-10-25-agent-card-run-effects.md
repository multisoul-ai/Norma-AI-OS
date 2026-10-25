# Agent 卡片运行特效:五套风格 + 实施任务

状态:设计稿,未实施。原型:[`docs/prototypes/agent-card-run-effects/index.html`](../prototypes/agent-card-run-effects/index.html)(风格对比);设置页原型:[`settings.html`](../prototypes/agent-card-run-effects/settings.html)。两页共用 `run-styles.js`。
参考 skill:`apple-design`(响应、可中断、克制)、`emil-design-eng`(动效服务反馈)、`animation-vocabulary`。落地后用 `review-animations` 审查。

## 1. 思路

不是“5 个状态各一个特效”,而是 **5 套完整的视觉语言**,每套覆盖同一条状态线:`Ready → Thinking → RunningTool → WaitingApproval → Done | Failed`(`AgentTaskState`,`crates/runtime/src/events.rs`)。结构与现有“派发特效款式”一致:用户在设置里选一套,默认保持现有表现(边框蛇 + 呼吸 + 完成柔光)。

语义色全风格统一:蓝=思考、青蓝=工具、琥珀=等你、绿=完成(未查看前保留)、红=失败。

## 2. 五套风格(v4)

保留磁流体与扫描;其余三套(v1–v3 的极光、声纹、萤火、声呐、神经突触、全息点阵、菌丝、义眼、等离子、鱼群)均已废弃。v4 的取向:克制、细线、材质与物理现象,避免卡通化与噪点堆叠。

| 风格 | 画在哪 | 隐喻 | 一句话 |
|------|--------|------|--------|
| A 灵光 Aura | 卡片边缘外侧 + 向内渗光 | 光穿过玻璃边缘的色散 | 一圈缓慢流转的光谱柔光,颜色即状态 |
| B 等高线 Contour | 卡片内部 | 会呼吸的数据地形 | 细等高线随地形漂移,峰顶标记指向“注意力” |
| C 磁流体 Ferrofluid | 卡片外缘一层膜 | 被磁场牵动的液态金属 | 保留 |
| D 声振 Cymatics | 卡片内部 | 振动板上的克拉尼沙图 | 沙粒随“频率”聚成不同对称图案 |
| E 扫描 Scanner | 卡片内部 | 终端扫描线 | 保留 |

### 状态 × 风格矩阵

| 状态 | A 灵光 | B 等高线 | C 磁流体 | D 声振 | E 扫描 |
|------|--------|----------|----------|--------|--------|
| Ready | 无光 | 近乎平坦的三四条淡线,极慢 | 薄膜紧贴 | 低振幅,沙粒稀疏落定 | 无 |
| Thinking | 蓝紫粉青光谱缓慢流转,沿周长明暗起伏 | 三座山丘漂移,等高线流动,峰顶标记跟随最高点 | 表面蠕动,小尖刺呼吸 | 每 2.6s 换一个振型,沙粒重新聚成新图案 | 扫描线 2s 一遍 |
| RunningTool | 流转加速,每次切换一束亮光从工具芯片处沿边缘疾行 | 工具芯片下隆起尖峰,切换时尖峰跳高 | 尖刺集中在标题栏一侧 | 切到高频振型,图案更细密,切换时沙粒一震 | 0.9s 一遍 + 小撕裂 |
| WaitingApproval | 转暖色(琥珀/桃/玫瑰),亮度随心跳呼吸 | 中心荡开同心波纹,振幅随心跳 | 底部液滴随心跳鼓动 | 固定振型,每次心跳沙粒轻颤 | 光标闪烁 |
| Done | 一道亮光绕周一圈后沉淀为薄荷绿柔光(未查看) | 地形收拢成一座平静圆丘,峰顶标记停在中心(未查看) | 尖刺退去,镜面高光绕一圈,绿光边(未查看) | 振动停止,沙粒落定成清晰对称图案(未查看) | 绿色扫描落底 |
| Failed | 不规则闪两三下后收缩成一圈暗红细光 | 短暂乱纹后地表塌陷成深坑 | 下垂滴落 | 剧烈震散后坍落到卡片底部 | 撕裂后留红线 |

GPUI 可行性:
- A 的柔光在原型里用低分辨率离屏放大实现;GPUI 无路径模糊,改用沿周长分段的圆角 quad + `box_shadow` 模糊(每段一色),段数 24–36,需在 T2 测成本。
- B 为 CPU 端 marching squares(约 44×28 网格 × 11 层),输出折线;只有可见卡片计算,状态权重平滑过渡保证可中断。
- D 有状态(700 粒子位置),存在 `NodeMotion`;随机抖动改为按粒子序号 + 时间片的 hash,保证可复现。屏外暂停。

### 极简三向(候选默认,低干扰)

全卡只动一个元素,其余静止;减少动态效果下保留颜色、去掉节奏。

| 状态 | 一个点 Dot(推荐) | 一条线 Hairline | 一道光 Underline |
|------|------------------|-----------------|------------------|
| Ready | 灰点 | 无 | 无 |
| Thinking | 蓝点 2s 慢呼吸 | 一段颜色沿 1px 边框缓慢流过 | 光段在标题栏分隔线上来回 |
| RunningTool | 每次切换外扩一圈细环 | 流速加快 | 光段停在工具名下方,切换时轻闪 |
| WaitingApproval | 琥珀点心跳双搏 | 整圈琥珀随心跳亮暗 | 整条线琥珀呼吸 |
| Done | 绿点 + 常驻细环(未查看) | 绿线从状态点绕一圈后停住(未查看) | 绿色自左向右铺满(未查看) |
| Failed | 红点静止 | 整圈红线 | 整条红线 |

实现成本最低:Dot 只画两个圆,Hairline 为分段描边,Underline 为一条渐变线段,均无粒子、无模糊。

### 共同规则

- **可中断**:所有强度、颜色用指数平滑(时间常数约 140ms)趋近目标,新状态从当前值起步,不重置。
- **未查看保留**:Done 的残留(底色/余烬/绿环)直到用户点击卡片或聚焦才淡出;Failed 残留保留到下一次任务。
- **减少动态效果**:五套统一降级为卡片底部 3px 状态色细条的 160ms 淡变,无位移、无粒子、无扫描。
- **高频克制**:工具切换只做小幅反馈(提亮/ping/小撕裂),同卡片 600ms 内最多一次。

## 3. 实现设计(GPUI 0.2.2)

- **纯函数沉底**:`crates/app/src/motion/run_style/`,每套一个文件(`aura.rs`、`contour.rs`、`ferrofluid.rs`、`cymatics.rs`、`scanner.rs`),≤500 行/文件。输入 `RunFrameInput { state, elapsed, since_ping, intensity, rgb, viewed, scale, seed }`,输出绘制图元列表(`Vec<RunPrim>`:圆、线、弧、渐变圆、矩形),零 GPUI 类型,可单测。
- **种子确定性**:粒子/撕裂位置由 `hash(task_ref, i, 时间片)` 派生,paint 不存粒子状态,重放与测试确定(同派发特效约束)。
- **状态**:`NodeMotion` 增加 `run_style_level`(平滑强度/颜色)与 `run_since_change`、`run_since_ping`;计入 `frame_synced_active` / `next_visual_boundary`;`ready` 且强度 < ε 时停止 notify。
- **绘制**:`cards/run_style_paint.rs` 把图元转成 `paint_quad` / `PathBuilder`;卡片内部类(A/B/D/E)按圆角矩形分区绘制,不依赖 `overflow_hidden()`(见 CLAUDE.md 圆角约束);外缘类(C)画在卡片外 margin 区(参考 `cards/border_glow.rs`)。渲染前先裁剪视口外节点。
- **GPUI 无加法混合**:辉光用多层半透明圆/描边近似;渐变圆(A、C)先测成本,过贵则退回 2–3 层同心圆。
- **预算**:单卡每帧 build+paint ≤ 0.4ms;同屏 > 6 张运行中卡片时,D 粒子 700→300、B 网格 8→12px、A 分段 36→24。
- **选择与存储**:风格是全局偏好(`SettingsStore` 键 `effects.run_style`,默认 `classic`=现状);设置页放入 设置 → 通用 → 特效,排在“派发特效”分区之后,结构与之同构:当前大预览(六状态自动轮播,状态芯片可固定)→ 风格库网格(极简 / 表现力两组,悬停播放,点击即生效 + 撤销)→ 行为开关(完成提示保留到查看、遵循系统减少动态效果)。未知值回落 `classic`。

## 4. 实施任务

> 本期只做 经典 / 一道光 / 灵光 / 磁流体 + 设置页,以 [`docs/exec-plans/2026-10-25-agent-card-run-styles.md`](../exec-plans/2026-10-25-agent-card-run-styles.md) 为准;下表为五套全量时的原始拆分,仅供参考。

顺序:T0 → T1 → (T2…T6 并行)→ T7 → T8 → T9。每任务先写失败测试。

| 任务 | 内容 | 主要文件 | 验证 |
|------|------|----------|------|
| T0 | 核对 `AgentTaskState` ↔ `BreathingIntent` 映射与 app 层消费 `AgentTaskUpdate` 的位置;结论写回本文 | `motion/mod.rs`、`motion/breathing.rs` | 无代码 |
| T1 | 骨架:`RunStyle` 枚举、`RunFrameInput/RunPrim`、平滑强度、ping 节流、`frame_synced_active` 接入、`classic` 回落 | `motion/run_style/mod.rs`、`motion/activity.rs`、`motion/visual.rs` | `cargo test -p app motion`:打断从当前值起步;ready 后停止 notify;ping 600ms 节流 |
| T2 | A 灵光 | `run_style/aura.rs` | 单测:调色板平滑过渡;工具亮光位置随时间单调;Failed 闪烁序列确定;分段 quad 数上限 |
| T3 | B 磁流体 | `run_style/ferrofluid.rs` | 单测:膜偏移非负;Approval 液滴位于底部中点;滴落长度上限 34px |
| T4 | B 等高线 | `run_style/contour.rs` | 单测:marching squares 四种情形;权重切换连续;峰顶标记位置 |
| T5 | D 扫描 | `run_style/scanner.rs` | 单测:扫描线位置周期、Approval 光标 530ms 闪烁、撕裂切片确定 |
| T6 | D 声振 | `run_style/cymatics.rs` | 单测:振型函数节点线;粒子收敛到节点;坍落停在底部;大 dt 不发散 |
| T7 | 绘制接入:`run_style_paint.rs` 图元→GPUI;屏外裁剪;圆角自绘 | `cards/run_style_paint.rs`、`cards/agent/card.rs`、`scripts/check-performance-guardrails.sh` | `cargo check -p app`;性能护栏;`check-max-file-lines.sh` |
| T8 | 设置:`agent_card.run_style` 偏好 + 设置页选择器与预览;后台线程保存 | `crates/app/src/settings/…`、`storage` | 设置状态单测;i18n 测试 |
| T9 | 真实 App 验收:desktop-e2e 新模块,五风格 × 六状态(深/浅 + 减少动态效果);`review-animations` 审查 | `test/desktop-e2e/Scenarios/<模块>/`、`checks/<模块>/` | `scripts/desktop-e2e.sh <模块>`;看图清单逐项判定,含“结束后无残留帧” |

## 5. 验收标准

1. 五套风格在六个状态均有对应表现,与原型一致;切换风格即时生效,无需重启。
2. 状态切换无跳变;减少动态效果下无位移/粒子/扫描。
3. Done 残留在查看后淡出;Failed 残留在下一任务开始时清除。
4. 6 张卡片同时运行不超预算;屏外卡片不计算特效帧。
5. 深浅主题均可辨;语义色与已有 Cool/Warm/Done 一致。

## 6. 风险

- 原型为 Canvas2D,GPUI 没有加法混合与高斯模糊,A 的分段阴影柔光与 D 的粒子数需在 T2/T6 先测深浅色观感与成本。
- T0 结论可能改变 Approval 与现有 Warm 呼吸的衔接方式。
- 默认保持 `classic` 是为了不改变现有用户体验;是否换默认风格需要产品决定。
