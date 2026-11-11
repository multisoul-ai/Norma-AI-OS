# 语音光晕风格化：第一期实施方案

- 日期：2026-11-10
- 范围：**经典、光环、声纹、星尘** 四种风格 + 设置 → 特效「语音光晕」区块 + 设置 → 语音 的迁移入口
- 不在本期：极光、绽放、流丝（原型保留，二期按同一接口追加）
- 原型（primary source，分支 `design/voice-glow-prototype-20261110`）：
  - 效果：`docs/prototypes/voice-glow/index.html?variant=0|A|B|C`
  - 设置页：`docs/prototypes/settings-effects/index.html#voiceGlow`
- 实现分支：`feat/voice-glow-styles-20261110`（从 `dev` 新开 worktree），**单个 PR 合入**

## 1. 验收标准

| # | 标准 |
|---|---|
| AC1 | 设置 → 特效 出现「语音光晕」区块：当前大预览、状态 filter chips（自动/聆听/思考/回复/打断）、5 张风格卡片（关闭、经典、光环、声纹、星尘）、强度分段按钮、「聆听与思考时也显示」开关 |
| AC2 | 选择风格立即作用于当前窗口；重启 App 后保留；所有项目共用 |
| AC3 | 选择后出现带「撤销」的状态行，⌘Z / 撤销恢复上一个风格 |
| AC4 | 老用户升级：旧开关开（或未设置）→ 经典；旧开关关 → 关闭。经典的表现与升级前逐像素一致 |
| AC5 | 光环、声纹、星尘在聆听、思考、回复三态有可区分的表现；打断时 80ms 收回 |
| AC6 | 「聆听与思考时也显示」关闭后，新风格只在回复时出现；经典风格不受此项影响，开关在经典下禁用并说明原因 |
| AC7 | 系统「减少动态效果」开启时：不流动、无粒子，只保留随音量变化的亮度 |
| AC8 | 设置 → 语音 中原「说话光晕」开关改为「语音光晕 · 当前风格 · 在特效中设置 ›」，点击跳到特效页该区块 |
| AC9 | 卡片、Command Dock、字幕始终可读；光效不拦截鼠标 |
| AC10 | 中英文完整；悬停/按下/焦点/选中/禁用状态齐全 |
| AC11 | 性能：语音活跃时帧时间不劣于经典风格 1.5 倍；静止时零额外绘制 |

## 2. 现状（已核对代码）

- 状态机 `VoiceWindowInset`（`crates/app/src/app/voice_window_inset.rs`，**498 行，已到上限**）：强度淡入 120 / 淡出 220 / 打断 80ms，能量 attack 35 / release 120ms。
- 它运行在独立实体 `VoiceWindowInsetView`（`voice_window_inset/view.rs`）里，通过 `VoiceWindowInsetEventBus`（`voice_window_inset/channel.rs`，原子量 + 队列 + wake 通道）接收 TTS 事件；12ms 一帧，只在可见变化时 `notify`。
- 开关来源：`voice.window_inset_enabled`（`settings/voice/state/values.rs:54`），经 `MultisoulApp::sync_voice_window_inset_enabled`（`app/state.rs:54`）写入 bus。
- 现有输入只有 TTS 播放；聆听/思考信号已在别处：`VoicePhase::Recording | Transcribing`（`app/voice/control.rs`）、`DoubaoStatus::Thinking`（`app/doubao.rs`）、GPT-Live `CallStatus`（`app/gpt_live/state.rs`）。
- 特效设置已有完整的「卡片运行特效」区块可照抄：`settings/effects/run_section.rs`（悬停预览、状态标签、撤销）、`app/run_effects_actions.rs`（保存/撤销）、`settings/effects/state.rs`（`EffectsSettingsState`，键 `effects.run_style`）。
- Dock 几何：`COMMAND_DOCK_BOTTOM_PX = 22`、`COMMAND_DOCK_HEIGHT_PX = 52`（`effects/dispatch_receipt.rs`），宽度随内容、上限 840。
- GPUI 0.2.2（`vendor/gpui-0.2.2`）可用绘制：`linear_gradient`（两色标）、`PathBuilder::{fill, stroke, curve_to, arc_to, add_polygon}`、`paint_path`、`paint_quad`、`paint_shadows`（`BoxShadow` 带 `blur_radius`）、`with_content_mask`。**没有径向/锥形渐变、没有 blur 滤镜、没有混合模式**。

## 3. 架构

### 3.1 分层

```text
crates/app/src/effects/voice_glow/          纯逻辑，无 GPUI 依赖以外的副作用，可单测
  mod.rs        VoiceGlowStyle { Off, Classic, Halo, Ribbon, Stardust }；id()/from_id()；DEFAULT = Classic
  phase.rs      VoiceGlowPhase { Idle, Listening, Thinking, Speaking } + 各语音源 → 相位的归一函数
  prefs.rs      VoiceGlowPrefs { style, gain: Gain(Soft|Standard|Vivid), show_listening_thinking }；旧键迁移
  frame.rs      GlowFrame { size, dock: Bounds, energy, intensity, phase, t, reduce_motion, gain }
  prim.rs       GlowPrim { Path{path, fill, alpha}, Shadow{bounds, radius, color, blur}, Quad{..} }
  styles/halo.rs、ribbon.rs、stardust.rs     fn build(&GlowFrame, &mut StyleState, out: &mut Vec<GlowPrim>)
  particles.rs  星尘固定容量粒子池（220），无帧内分配

crates/app/src/app/voice_window_inset/      现有实体，改为“状态机 + 按风格出图”
  state.rs      从 voice_window_inset.rs 迁出的 VoiceWindowInset（P1 纯搬移）
  classic.rs    迁出的 inset_partition / 渐变 / 角缝补偿（经典风格，逻辑不变）
  paint.rs      GlowPrim → window.paint_path / paint_shadows / paint_quad
```

风格只产出 `GlowPrim`，不碰 `Window`，所以几何、透明度、粒子数都能在单测里断言；`paint.rs` 是唯一调用 GPUI 绘制的地方。

### 3.2 数据流

1. **偏好**：`EffectsSettingsState` 新增 `voice_glow: VoiceGlowPrefs`（键 `effects.voice_glow.style` / `.gain` / `.phases`），与 `run_style` 同一缓存。`sync_voice_window_inset_enabled` 改名为 `sync_voice_glow`，把 `style / gain / show_listening_thinking / window_visible` 写进 bus（新增一个 `Mutex<VoiceGlowPrefs>` 字段，变化时 wake）。
2. **相位**：`frame_loop.rs` 每帧汇总三个语音源得到 `VoiceGlowPhase`，变化时写 bus；`show_listening_thinking = false` 或风格为经典时，聆听/思考映射为 `Idle`。
3. **能量**：回复沿用 TTS 的 `PcmEnergyTimeline`；聆听用麦克风 PCM 走同一个 `PcmEnergyTimeline::from_pcm`，经 bus 新事件 `MicEnergy(timeline)` 送入。语音指令模式若拿不到麦克风帧，聆听退化为固定 0.4 能量。
4. **Dock 位置**：Dock 渲染处加一个零尺寸 `canvas` 探针，prepaint 时把 bounds 写入 bus（与上次相同则不写）；拿不到时按 `窗口宽度居中、宽 min(840, w-36)、底 22、高 52` 估算。
5. **绘制**：`VoiceWindowInsetView::render` 由「只画经典」改为「按 style 调用 build → paint」；风格为 `Off` 时与禁用等价，不启动帧驱动。

### 3.3 每个风格在 GPUI 里的画法（对应原型）

| 风格 | 原型做法 | GPUI 0.2.2 做法 |
|---|---|---|
| 经典 | 四边白色线性渐变 | **不改**，现有 `inset_partition` + 角缝补偿整体迁入 `classic.rs` |
| 光环 | 锥形渐变描边 + blur 三层 | 四条边各切 N=12 段，每段一个 `paint_shadows`（贴边外侧的细矩形，`blur_radius` 22–70 随能量，颜色取 6 色色环按相位旋转）= 外层柔光；中层同法 blur 7–21；内层 2px 用 `PathBuilder::stroke` 沿圆角矩形、按段着色。色带流动只改色环相位 |
| 声纹 | 三条正弦闭合填充 + blur | 每条丝带 96 点闭合 `PathBuilder::fill`，半透明叠加三色；发光层 = 同一路径放大 1.06 倍、alpha 0.25 再画一次；中心白线 `stroke(1px)`。聆听为单条细线，思考为左右移动的小波包 |
| 星尘 | 粒子 + 小 blur | 每颗粒子：核心 `paint_quad` 圆角 = 半径；光晕 `paint_shadows` blur 4–6。Dock 上方柔光用一个宽椭圆圆角矩形 + 大 blur shadow。粒子池上限 220 |

没有混合模式的补偿：深色壁纸上原型用 screen，GPUI 改为普通 alpha 叠加，颜色整体提亮一档（色值写在各风格常量里），P4 用真实 App + 默认壁纸 + 一张亮色壁纸校准。

### 3.4 设置页（照原型 `settings-effects/index.html#voiceGlow`）

- 新文件 `settings/effects/voice_glow_section.rs`（标题 + hero + 状态 chips + 网格）与 `voice_glow_options.rs`（强度、开关两行），在 `settings/effects/view.rs` 的 `run_section` 之后追加。
- **预览**：hero 与卡片都用同一套 `effects::voice_glow` 在缩小坐标系里出图，背景用当前壁纸静帧（`DEFAULT_WALLPAPER_ASSET` 或当前选择的首帧）+ 两张示意卡片 + Dock 胶囊，与 run_section 的迷你画布同构。卡片静止时画固定能量 0.7 的定格帧，悬停 250ms 或键盘聚焦时才播放；系统减少动态效果时只在点「重播」后播放。
- 状态 chips：自动按 聆听 2.6s → 思考 1.8s → 回复 4.8s 循环，能量用确定性的模拟包络（固定种子，便于截图稳定）。
- 动作：新增 `app/voice_glow_actions.rs`，照 `run_effects_actions.rs` 实现 `pick_voice_glow_style / undo_voice_glow_style / set_voice_glow_gain / toggle_voice_glow_phases`，保存走后台执行器，状态行文案用 `LocalizedText`。
- 语音页：`settings/voice/view/spoken_reply.rs` 的 `voice.speech_glow` 开关行换成跳转行（显示当前风格名）；点击切到特效页并滚动到该区块。旧回调 `on_voice_settings_toggle_window_inset` 删除。
- i18n：`crates/ui/locales/effects.{en,zh-CN}.json` 新增 `effects.voice_glow.*`（标题、说明、所有项目共用、5 个风格名 + 一句话、5 个状态、强度三档、开关及两种提示、状态行、撤销）；`voice.*` 删除 `speech_glow` 两个键，新增跳转行文案。

## 4. 实施步骤（同一分支、同一 PR，按步骤提交）

| 步骤 | 内容 | 先写的失败测试（TDD） | 验证 |
|---|---|---|---|
| **P0 设计记录** | 按 CLAUDE.md §8 加载 `apple-design`、`emil-design-eng`，补 §6 设计记录 | — | 文档评审 |
| **P1 拆分** | `voice_window_inset.rs` 拆为 `state.rs` + `classic.rs`，`view.rs` 不变；纯重构 | 无新测试；现有 `voice_window_inset_tests` 必须原样通过 | `cargo test -p app voice_window_inset`；`check-max-file-lines.sh` |
| **P2 偏好与迁移** | `VoiceGlowStyle`、`VoiceGlowPrefs`、旧键迁移、bus 传偏好；经典/关闭两档可用（等价现状） | 迁移三例（开/关/未设置）；`from_id` 未知值回落经典；bus 设为 Off 时 `is_animation_active() == false` | `cargo test -p app voice_glow` |
| **P3 设置页** | 特效页区块（先只有关闭/经典两张卡）、动作与撤销、语音页跳转行、i18n | 动作：pick → 保存 → undo 恢复；语音页跳转定位 | `check-i18n.py`、`test-i18n.py`；desktop-e2e 新场景 `SettingsVoiceGlow`（`--visual`）：区块出现、选择关闭→经典、撤销、语音页跳转 |
| **P4 相位与能量** | 三个语音源 → 相位；麦克风能量；`show_listening_thinking` 生效 | 相位映射表驱动单测（每个语音源每个状态）；经典在聆听为 Idle；打断 80ms 淡出 | 单测；现有 voice e2e 不回归 |
| **P5 星尘** | 粒子池 + 绘制；加入设置网格 | 能量 0 不生成；池满不增长；减少动态效果粒子数 = 0；帧内无分配（容量不变断言） | desktop-e2e `voice` 模块新场景 `VoiceGlowStyles`：默认壁纸下聆听/思考/回复各一张 `--visual` |
| **P6 声纹** | 三丝带 + 聆听单线 + 思考波包 | 包络两端振幅为 0；能量 0 振幅为基线；减少动态效果相位不推进 | 同上场景追加声纹三张；字幕不被遮挡（看图） |
| **P7 光环** | 分段 shadow 光环 | 段数与颜色相位；厚度随能量单调；聆听厚度 < 回复 | 同上场景追加光环三张；性能对比经典 |
| **P8 动效审查** | 加载 `review-animations` 审查四种风格与设置页动效 | — | 审查结论写入 §6；修正直接在本分支追加提交 |

全部步骤在一个分支上完成、一个 PR 合入 `dev`；每个步骤一个或几个 commit，保证每个 commit 都能编译、测试通过，便于 review 时逐步看、出问题时定位。

顺序理由：P1–P3 先把拆分、迁移和设置页做稳（此时用户可见效果不变）；新风格按「几何最简单 → 最依赖模糊」排列，光环还原风险最大，放最后，开工前先出一张 GPUI 截图与原型对比。

合并前（PR 门禁）：
- `cargo test -p app`、`cargo check -p app`
- `check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh`、`check-i18n.py`、`test-i18n.py`
- desktop-e2e：`SettingsVoiceGlow` 与 `VoiceGlowStyles`（`--visual`），深色 + 浅色
- `review-animations` 审查结论（P8）与对抗式 code review 结论写入 PR 描述
- PR 最新提交的 CI（含 `rust checks`）全部通过后再合并

## 5. 验收映射

| AC | 证据 |
|---|---|
| AC1、AC3、AC8、AC10 | `SettingsVoiceGlow`（desktop-e2e，`--visual`），浅色 + 深色各跑一次；i18n 脚本；中英文各一张截图 |
| AC2 | `SettingsVoiceGlow`：选择星尘 → 关闭设置 → 触发 TTS → 截图；重启 App 后再截图 |
| AC4 | P2 迁移单测；P1 像素对比（现有 e2e 视觉基线） |
| AC5、AC6 | P4 相位单测 + `VoiceGlowStyles` 每风格三态截图；关闭开关后聆听截图无光 |
| AC7 | 各风格 `reduce_motion` 单测；开系统减少动态效果跑一次 `VoiceGlowStyles` |
| AC9 | `VoiceGlowStyles` 看图检查卡片/字幕；光效层 `pointer-events` 等价：overlay 不注册鼠标事件（单测断言无 `on_mouse_*`） |
| AC11 | `check-performance-guardrails.sh`；performance_monitor 在 VoiceActive 下记录四种风格的帧时间，写入 PR |

## 6. 设计记录（P0 填写）

待填：参考的 skill、采纳/拒绝的关键设计决策、对应 GPUI 实现与验证方式。已确定的：
- 设置页菜单与选择器遵循 Material（M3 filter chips、带勾分段按钮、单选卡片 + 撤销 snackbar），其余遵循 Apple HIG。
- 动效目的：光效表达「谁在说、说多大声」，不是装饰；打断必须 80ms 内无回弹收回；减少动态效果下保留亮度反馈。

## 7. 风险

| 风险 | 缓解 |
|---|---|
| 光环用 shadow 近似锥形渐变，段间可能有色阶 | 段数可调（12→24）；段与段 shadow 重叠 30%；P7 前先出一张 GPUI 截图和原型对比再决定是否降级为单色光环 |
| 无混合模式，亮壁纸上光效发灰 | 每个风格在一张亮壁纸上补验收截图；必要时按壁纸平均亮度自动提高 alpha（二期） |
| Dock 宽度随内容变化，探针一帧滞后 | 只影响声纹/星尘的起点，滞后一帧不可感知；探针写入去重 |
| 三种语音模式的麦克风数据通道不同 | P4 先接 GPT-Live 与豆包；语音指令模式聆听用固定能量 |
| 设置页卡片预览成本 | 只有悬停中的一张卡片和 hero 在动；其余是定格帧，壁纸用静帧不解码视频 |

## 8. 实施记录（实现分支 `feat/voice-glow-styles-20261110`）

与方案的差异（均已在代码里落实）：

- 预览的经典风格用四条边柔光近似；真实窗口里的经典仍是原分区渐变，逻辑原样迁入 `app/voice_window_inset/classic.rs`。
- 设置页缩略图/大预览用同一套图元，背景取内置默认壁纸静帧（`DEFAULT_WALLPAPER_ASSET`），不读当前壁纸。
- 聆听能量：未接入麦克风 PCM，聆听时用 0.35 ± 0.15 的轻微呼吸能量（`app/voice_window_inset/glow.rs::LISTENING_BASE`）；GPT-Live 的聆听/思考相位也未接入，目前只接了语音指令（录音/转写）与豆包（聆听/思考）。
- 语音页入口只切到特效页，未滚动到「语音光晕」区块。
- 减少动态效果跟随系统与 App 设置（`motion::system_reduce_motion`），没有单独开关。
