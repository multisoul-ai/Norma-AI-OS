# 派发特效可切换 + 设置页「特效」分区

状态：S0–S6 已实现，设置页与实际派发已有真实 App 证据；S7（统计）未做。提交时基于 `origin/dev` 的 `5ac797a5` 集成，保留 Norma 大脑设置入口。下文 §12 为历史施工记录，最新行为与验收范围以本节为准。视觉与交互以两份原型为准，本文只定义生产实现的分层、切片与验收：

- 特效本身：[`docs/prototypes/dispatch-effects/index.html`](../prototypes/dispatch-effects/index.html)（流星、纸飞机、声纹、萤火、烟花、闪电、星漩）
- 设置页：[`docs/prototypes/settings-effects/index.html`](../prototypes/settings-effects/index.html)（Material 3，结构同壁纸页）

## 当前行为修订（2026-10-24）

用户要求派发始终使用选定的完整特效：已移除连续派发、系统及环境“减少动态效果”触发的脉冲降级，删除脉冲款式与设置页两个降级开关。旧偏好中的 `pulse` 按未知款式回到来源默认（语音/想法流星、Agent 交接离子）；历史简化开关键不再读取。播放区仅保留速度。系统无障碍偏好仍可暂停设置页自动预览，点“重播”播放完整款式，不影响实际派发。原有排队、去重、并发与容量边界保留。下文中 S2/S5/S6/S8 关于脉冲和简化开关的描述是修订前记录，以本节为准。

本次验证：派发相关单测 79 条、设置状态单测 8 条、UI 单元/国际化测试 13 条全部通过；`cargo check -p app --locked`、`cargo build -p app --locked`、`cargo build -p app --release --locked`、格式/行数/no-allow/性能护栏通过。真实设置页 `settings-dispatch-effects` 8/8 步骤通过（93s），证据 `artifacts/desktop-workspace-e2e/2026-10-24T17-19-38Z/report.json`；已核对深浅主题播放区截图，只保留速度。实际派发在 `MULTISOUL_REDUCE_MOTION=1` 下跑 `workspace-dispatch-effects`，6/6 步骤通过（84s），证据 `artifacts/desktop-workspace-e2e/2026-10-24T17-26-49Z/report.json`；烟花/闪电/星漩可见活动跨度分别为 1505/302/848ms，三条想法均已指派。视觉核对确认烟花爆发、闪电分叉、星漩飞行均使用完整款式。

实际派发场景首次运行因旧测试的固定基线差分误判失败（`artifacts/desktop-workspace-e2e/2026-10-24T17-22-07Z/report.json`）：最后五张截图逐字节相同，仍被计为持续动画。已改成与收尾帧差分，使用单调时钟记录真实采样时间，并断言最后两帧已静止；修正后重跑通过。后续 review 发现“最后两帧静止”仍会放过冻结残留；提交时新增独立派发前基线的消失断言（排除假终端顶部文字区，覆盖飞行走廊与卡片中心）及 400 像素冻结残留回归测试。新版场景的运行结果在提交验收记录中补充。

## 1. 目标与非目标

目标：派发反馈从固定的流星变为可选款式；用户在 设置 → 通用 → 特效 中按工作空间/项目选择特效，可按派发来源（语音、想法清单、Agent 交接）单独覆盖，可调播放速度、连续派发简化与减少动态效果；设置页内可即时预览。

非目标（一期不做）：用户自定义/导入特效、音效、手机端设置、每张卡片单独设置、粒子丰富度档位。

## 2. 现有能力与复用（已核对代码）

| 需要 | 已有 | 用法 |
| --- | --- | --- |
| 特效生命周期、排队、去重、减少动态效果 | `motion/dispatch_receipt.rs`：`DispatchEffectStore::spawn/tick`、`MAX_ACTIVE_EFFECTS=3`、`LAUNCH_STAGGER`、`REDUCED_PULSE_DURATION` | 不另造调度；每个 effect 在 spawn 时记录款式 |
| 飞行时长 | `FLIGHT_DURATION` / `IDEA_FLIGHT_DURATION` 常量 + `DispatchVisualOrigin::uses_idea_meteor` | 改为由款式提供时长；`uses_idea_meteor` 由款式解析取代 |
| 绘制入口 | `effects/dispatch_receipt.rs::build_paint_plan` 按阶段分发到 `flight.rs` / `impact.rs`，原语为 `PathBuilder` + `PaintQuad` | 改为按款式分发；现有流星、离子画法原样迁入款式模块 |
| 起点类型 | `DispatchVisualOrigin::{CommandDock, IdeaRow, AgentNode}` | 起点 → 来源：语音 / 想法清单 / Agent 交接 |
| 派发调用点 | `app/agent_runtime/dispatch_effects.rs`、`app/agent_runtime/idea_commands.rs` 调 `spawn_dispatch_receipt(_from)` | 两处传入解析后的款式 |
| 范围存储 | `desktop_workspaces.wallpaper_json`、`desktop_project_areas.wallpaper_json`（`storage/src/desktop_workspace_store/wallpaper.rs`），NULL = 跟随 | 新增同构列，见 §4 |
| 全局偏好 | `storage::SettingsStore` 字符串键值 | 播放类开关 |
| 设置页骨架 | `settings/navigation.rs` 的 `SettingsSection` 与 `GENERAL_SECTIONS`、`settings/content.rs` 路由、`settings/wallpaper/{scope_view,current,layout,cells}.rs` | 结构照搬壁纸页；`WallpaperLayout` 的列计算直接复用 |
| 菜单 | `docs/design-docs/material-menus.md` 约定 + `ui::component` Popover/List | 「按派发来源」三个选择器 |
| 绘制诊断 | `frame_diagnostics::DispatchEffectPaintReport` | 每款的性能预算用它验收 |
| 真实 App 预览钩子 | `NORMA_VISUAL_PREVIEW=dispatch-receipt-flight` 等（`e2e-test-support`） | 扩展为按款式定格 |

## 3. 分层与依赖

```text
storage   DispatchEffectPreference（serde，仅字符串 + 校验）；列读写；全局键
app/motion  DispatchEffectStyle / EffectTiming / resolve_style()：纯函数，可单测
app/effects  dispatch_receipt/styles/<款式>.rs：每款一个模块，纯绘制
app/settings effects/：设置页视图与状态；保存走 background_executor
```

- `storage` 不认识款式枚举，只存字符串 id 并校验长度/字符集；未知 id 读回时由 `app` 回落到流星，不报错（前向兼容旧版本写入的新款式）。
- 款式解析在派发路径上只读内存缓存，不碰 SQLite（`check-performance-guardrails.sh` 约束）。缓存随设置保存、工作空间/项目切换刷新。

## 4. 数据模型

`desktop_workspaces` 与 `desktop_project_areas` 各加一列 `dispatch_effect_json TEXT`（迁移写在 `storage/migrations/desktop.rs`，与 `wallpaper_json` 同法），NULL 表示跟随上一层：

```json
{ "style": "ribbon", "sources": { "voice": null, "idea": null, "handoff": "ion" } }
```

- `style`：`meteor | plane | ribbon | firefly | fireworks | lightning | vortex | random | pulse`。
- `sources.*`：NULL = 跟随 `style`；非空为该来源的覆盖。
- 解析顺序：项目（非 NULL 且未开启“跟随工作空间”）→ 工作空间 → 内置默认。来源覆盖在选定的那一层内生效。
- 内置默认保持现状：`style = meteor`，`handoff = ion`（现有 Agent 间离子画法，作为交接来源的专属选项“离子”出现在菜单里，不进网格）。

全局键（`SettingsStore`）：`effects.speed`（`relaxed 1.33 | standard 1.0 | brisk 0.75`）、`effects.simplify_burst`（默认开）、`effects.follow_system_reduce_motion`（默认开）。

## 5. 渲染设计约束

1. **绘制是 (帧几何, 进度, 种子) 的纯函数**。paint 阶段不保存粒子状态：烟花用阻力 + 重力的解析解（`x = x0 + v0/k·(1−e^{−kt})`，`y` 另加 `g/k·t − g/k²·(1−e^{−kt})`），闪电折线、萤火落点、星漩参数都由 `task_ref` 派生的种子经 hash 生成。这样重放、暂停、预览定格、测试都确定。`MAX_PARTICLES_PER_EFFECT` 的预生成数组只留给流星。
2. **GPUI 没有加法混合**。原型深色主题用了 `lighter`，生产只能 alpha 叠加：用多层半透明描边/圆模拟辉光，并在 S0 对比深色下的观感。柔光点优先用带模糊阴影的圆形 quad，S0 测成本，过贵则退回 2–3 层同心圆。
3. **尺寸可缩放**。`DispatchEffectFrame` 增加 `scale`，所有线宽、半径、偏移乘 `scale`，设置页缩略图（约 0.6）和画布（1.0）共用同一套函数。
4. **预算**：单个特效每帧 build + paint ≤ 1ms（M1，Release），3 个并发 ≤ 2.5ms；路径数上限按款式写成常量并测试（烟花最多 100 颗星 × 7 段，超预算先砍尾迹段数）。
5. **款式在 spawn 时固定**：飞行中改设置不跳变；`random` 在 spawn 时按 `task_ref` hash 选出，去重与测试可复现。
6. **连续派发简化**：同一窗口 1s 内第 2 次及以后的 spawn，若开关开启，款式改为 `pulse`（复用现有减少动态效果的脉冲）。
7. **减少动态效果**：`reduce_motion = env 覆盖 || (follow_system && 系统设置)`，生效时所有款式走脉冲。现在只读环境变量（`motion::reduce_motion_from_env_value`），S6 补系统设置读取。

## 6. 切片与顺序

每片独立可合并，先写失败测试再实现，单文件 ≤ 500 行，不用 `#[allow]`。

**S0 技术探针（半天以内，结论写回本文）**
- 在 `gpui =0.2.2` 下画 30 个带模糊阴影的圆形 quad 的耗时，对比 3 层同心圆；
- 深色主题下 alpha 叠加的流星截图，和原型对比；
- 系统“减少动态效果”在现有依赖下的读取方式（`NSWorkspace.accessibilityDisplayShouldReduceMotion` 与通知）；
- App 内是否已有带操作的 toast/snackbar，没有就用壁纸页的保存状态行加“撤销”按钮。

**S1 存储**
迁移加两列；`DispatchEffectPreference` 的 serde、校验与读写；全局三个键。测试：迁移幂等、NULL 跟随、未知字符串原样读回、超长/非法字符拒绝、删除工作空间级联（列随行删除）。

**S2 motion：款式与时长**
新增 `DispatchEffectStyle`、`EffectTiming { flight, impact }`（取原型的阶段时长）与速度倍率；`DispatchEffect` 记录 style；`flight_duration` / `finished` 改读 timing；burst 简化与 random 解析。测试放 `crates/app/tests/unit/dispatch_effect_style_tests.rs`：每款时长、倍率、random 同 key 同结果、1s 内第二次变 pulse、reduce_motion 一律 pulse、飞行中不变款。

**S3 款式解析与调用点**
纯函数 `resolve_dispatch_style(&EffectPrefs, origin) -> DispatchEffectStyle`；app 层缓存 `EffectPrefs` 并在保存/切换时刷新；`dispatch_effects.rs`、`idea_commands.rs` 两个调用点传入款式。测试：三种来源 × 项目/工作空间/默认 × 覆盖/跟随的组合表。

**S4 绘制（拆成三片，每片两款）**
先把现有流星（`meteor.rs`）和离子（`flight.rs` 中交接画法）迁到 `effects/dispatch_receipt/styles/`，行为不变（现有测试全绿）。然后：
- S4a 纸飞机、声纹
- S4b 萤火、星漩
- S4c 烟花、闪电

每款的单测：起点/终点落在帧几何上、进度单调、同种子输出一致、路径/quad 数不超上限、`scale` 生效、阶段边界（landAt 前后）正确。每片合并前跑 `review-animations` 审查，并用 S8 的定格预览看真实 App。

**S5 设置页「特效」**
- `SettingsSection::Effects`，排在 `GENERAL_SECTIONS` 的 Wallpaper 之后；新增 `icons/settings-nav-effects.svg`；`content.rs` 路由。
- `settings/effects/{view,state,scope,current,cells,sources,playback}.rs`：范围分段按钮 + 跟随工作空间（同壁纸页 `scope_view`）；当前特效大预览；9 格网格（7 款 + 随机 + 仅脉冲）复用 `WallpaperLayout`；来源选择器按 `material-menus.md`（48px 选项、勾选表示已保存值、↑↓ 浏览、Enter 确认、Esc/Tab 关闭并回焦点）；播放区三项。
- 预览：网格静止时画定格帧（每款一个固定进度）；只有悬停/键盘聚焦的格子和当前大预览在动，且仅在设置窗口可见、该分区激活时请求帧；减少动态效果时大预览只在点“重播”后播放。
- 保存走 `background_executor`，UI 先乐观更新，失败回滚并在状态行提示；撤销恢复上一个值。
- i18n：`settings.zh-CN.json` / `settings.en.json` 新增键。
- 测试：状态机（选择、撤销、跟随切换、保存失败回滚）、来源菜单键盘浏览不改值、布局列数与壁纸一致。

**S6 系统减少动态效果**
按 S0 结论读取系统设置并监听变化，和 `effects.follow_system_reduce_motion`、环境变量组合。测试覆盖组合真值表（系统读取本身用真实 App 验证）。

**S7 统计（可选）**
`analytics` 新增 `effects_style_changed { style, scope }`，只发枚举值；不上传工作空间名或路径。

**S8 验收工具**
- `NORMA_VISUAL_PREVIEW` 增加 `dispatch-style-<id>-flight` / `-impact` 定格值；
- desktop-e2e：`test/desktop-e2e/Scenarios/settings/SettingsDispatchEffects.swift`，登记到 `SettingsModule.swift`，看图清单放 `checks/settings/`；画布侧在现有 canvas 模块加按款式定格的场景。

## 7. 验收标准与证据

| 标准 | 证据 |
| --- | --- |
| 默认行为与现在一致（语音/想法流星、交接离子） | S2/S3 单测 + 现有 `dispatch_*` 测试全绿 |
| 选择后下一次派发使用新款式，飞行中的不变 | S2 单测 + 真实 App 派发 |
| 项目覆盖、跟随工作空间、来源覆盖按 §4 顺序生效 | S3 组合表单测 |
| 重启后设置保留 | S1 存储测试 + 真实 App 重启 |
| 七款画面与原型一致，浅/深主题可读 | S8 定格截图 desktop-e2e，逐款看图 |
| 连续派发简化、减少动态效果回落为脉冲 | S2 单测 + 真实 App |
| 设置页 hover / pressed / 键盘焦点 / 选中 / 禁用状态齐全 | desktop-e2e 设置场景（浅/深） |
| 来源菜单键盘操作符合 `material-menus.md` | 单测 + 真实 App |
| 性能预算（§5.4） | `DispatchEffectPaintReport` 日志，Release 构建 3 并发实测 |
| 派发路径不读 SQLite | `check-performance-guardrails.sh` + S3 代码审查 |

每片合并前：`cargo test -p app`（受影响测试）、`cargo test -p storage`（S1）、`bash scripts/check-max-file-lines.sh`、`check-no-allow.sh`、`check-performance-guardrails.sh`；代码审查按 `agent-execution-policy.md` 用独立 subagent 做对抗式审查。

## 8. 设计记录

参考的 skill 与文档：`apple-design`（基线：内容优先、即时反馈、可访问性）、`docs/design-docs/material-menus.md`（菜单与选择器）、`emil-design-eng`（动效频率与可中断性）、`review-animations`（S4 每片合并前）。

采纳：
- 页面结构完全对齐壁纸页（范围 → 当前 → 库），用户不需要学新模式；
- Material 3 的胶囊导航指示、带勾分段按钮、选中卡片主色描边 + 勾标、48px 菜单项、撤销；颜色沿用 Norma 主题 token；
- 网格静止显示定格帧，悬停/聚焦才播放（渐进展示，避免 9 个动画同时跑）；
- 高频场景的退路：连续派发简化、仅脉冲、减少动态效果。

拒绝：
- 加法混合与 Web 风格模糊（GPUI 0.2.2 不支持，改用叠层）；
- paint 阶段有状态的粒子模拟（改为解析解 + 种子，可复现可测试）；
- 只有全局设置（壁纸已按工作空间/项目区分，特效保持一致）；
- 粒子丰富度档位（一期用固定预算和自动简化代替）。

## 9. 已确认决策（2026-10-24）

- **交接来源默认值**：保留现有离子画法为默认（§4）。
- **随机款式**：从全部 7 款中选，不排除闪电等强刺激款。
- **烟花不限频**：每次派发都完整播放，不做“当天首次”之类的限制；超出 §5.4 预算时只通过减少尾迹段数和星数来压成本。

## 10. 风险

- **深色观感**：没有加法混合，烟花、星漩在深色下可能不如原型亮；S0 截图后决定是否调高叠层透明度或改配色。
- **烟花成本**：最重的一款且不限频，S4c 必须在 Release 下实测 3 并发烟花满足 §5.4，达不到则先降尾迹段数（7 → 4），再降星数（100 → 64）。
- **系统减少动态效果读取**：依赖 S0 结论，若需要新增 macOS 绑定依赖，先确认与现有依赖不冲突。

## 11. S0 结论（代码核对部分，2026-10-24）

- **系统“减少动态效果”**：`motion/util.rs` 目前只认环境变量。`crates/app/Cargo.toml` 已依赖 `objc2-app-kit 0.3`，S6 只需给它加 `NSWorkspace` feature，读 `accessibilityDisplayShouldReduceMotion`，无需新增依赖。
- **提示条/撤销**：App 里只有工作台专用的 toast（`drawers/content/workbench/view.rs`），没有通用 snackbar。S5 复用壁纸页的保存状态行（`wallpaper-save-feedback` 那一行），在行内放“撤销”文字按钮，不新造全局组件。
- **发光**：`effects/dispatch_receipt/primitives.rs` 只有 `round_quad` / `outline_quad` / 路径，没有模糊。S4 先用 2–3 层同心半透明圆；带模糊阴影的圆是否更好看、成本多少，作为 S4a 的第一个基准，用 `DispatchEffectPaintReport` 在真实 App 里实测，不在这里凭猜测下结论。
- **未验证**：深色主题叠层观感、模糊阴影耗时，都需要跑真实 App，留到 S4a 实测。

## 12. 进度记录

- **S1 完成**：`crates/storage/src/desktop_workspace_store/dispatch_effect.rs`，测试 `crates/storage/tests/desktop_dispatch_effect.rs`。
- **S2 完成**：
  - 新增 `crates/app/src/motion/dispatch_style.rs`（`DispatchEffectStyle`、`timing(speed)`、`from_id`、`default_for`、`random_for`）；时长表与原型一致，流星/离子沿用原 900/430、480/430ms，未显式指定款式时行为不变。
  - `DispatchEffectStore` 新增速度倍率与连续派发简化（`BURST_WINDOW` = 1s，重复派发不计入）；`MotionStore` 新增 `spawn_dispatch_receipt_styled`、`set_dispatch_speed`、`set_dispatch_simplify_burst`。简化开关在 `MotionStore::new` 默认关闭，由 S3 按设置打开，所以现有测试不受影响。
  - `DispatchEffectPresentation` 新增 `style` 字段，供 S4 的绘制按款式分发。
  - 为了让 `motion/mod.rs` 保持 ≤500 行，把 `consume_repaint_interval` 挪到 `motion/util.rs`。
  - 测试 `crates/app/tests/unit/dispatch_effect_style_tests.rs`：时长表与倍率、id 往返、随机固定且覆盖 7 款、默认款式不变、飞行时长随款式与倍率变化、连续派发窗口、减少动态效果回落脉冲。
- **S2 遗留（交给 S3）**：`id`/`from_id`/`random_for`/`set_dispatch_*` 目前没有调用方，会有 dead-code 警告，S3 接入后消失。
- **既有问题（与本计划无关，未改）**：
  - `--features e2e-test-support` 编译失败：`app/e2e/visuals.rs` 对 `DispatchVisualOrigin` 的 `match` 没覆盖 `IdeaRow`。S8 要用 e2e 特性，到时一并修并把 `style` 加进快照。
  - `app::command_dock::tests::command_dock_pending_preview_does_not_block_new_commands` 在干净的 `HEAD` 上同样失败（预期报错“Finish or cancel the open file picker first”，实际为空），与本计划无关，未处理。
- **S3 完成**：`StyleSelection`（项目覆盖 → 工作空间 → 默认；来源覆盖；随机在派发时按 `task_ref` 固定）接进 `MotionStore::spawn_dispatch_receipt_from`，两个派发入口无需改动。应用层用 gpui `Global`（`EffectsSettingsState`）缓存，`sync_all_dispatch_effects` 把解析结果推给宿主与各项目会话（按 `(工作空间, 项目)` 键）；会话创建后与 `bind_desktop_wallpaper` 末尾都会同步。派发路径不读 SQLite。
- **S4 完成**：`effects/dispatch_receipt/styles/`：`painter.rs`（向量、缓动、确定性哈希、弧长折线、画笔）+ 纸飞机、声纹、萤火、烟花、闪电、星漩各一个文件；流星/离子沿用旧实现，按 `presentation.style` 分发。每款都是 (几何, 时间, 种子) 的纯函数。
  - 性能（Debug-optimized，单帧 build）：最大图元数 纸飞机 25、声纹 34、萤火 126、烟花 291、闪电 56、星漩 174（预算 450）；build 最长 0.18ms。GPU 绘制耗时未量。
  - 辉光用 3 层同心半透明圆，没有加法混合；深色下亮度比原型低，已在真实窗口里看过。
  - 回归：声纹/闪电在缩略图小卡片上 `clamp` 区间反转会 panic（真实 App 崩溃后发现），已加 `clamp_into` 与 `tiny_preview_geometry_never_panics`。
- **S5 完成**：`SettingsSection::Effects`（通用组，壁纸之后），`settings/effects/{state,page,ui_state,preview,scope,current,grid,sources,playback,view}.rs`；保存在后台、乐观更新、失败回滚、可撤销；撤销“首次设置”需要 `clear_workspace_dispatch_effect`（e2e 中发现，已加存储测试）；保存成功后状态行收尾为 “Saved”。i18n：`effects.*`（zh-CN/en），占位符为 `%{name}`。
- **S6 完成**：`motion::system_reduce_motion()` 读 `NSWorkspace.accessibilityDisplayShouldReduceMotion`（`objc2-app-kit` 加 `NSWorkspace`/`NSAccessibility` feature）；只作用于派发特效；设置变化在下一次同步（设置保存、上下文切换）时生效，不实时监听。
- **S8 部分完成**：
  - `settings-dispatch-effects`（settings 模块）真实 App 7/7 通过：选款式、撤销、语音来源菜单、速度与简化开关、浅色主题、重启保持；证据 `artifacts/desktop-workspace-e2e/2026-10-24T15-04-54Z/`。之后又加了 “Saved” 状态断言和状态行修复，**这两处尚未在真实 App 重跑**（屏幕锁定）。
  - `workspace-dispatch-effects`（workspace 模块）：启动前写入烟花、`⌃1` 派发、设置页实时切闪电/星漩、按动画时长差异断言；**尚未跑通**，需解锁屏幕后重跑。
  - `--features e2e-test-support` 的编译错误（`IdeaRow`）已修复，并把 `style` 加进派发快照。
  - 这台机器的 OCR 读不了中文，新场景全部用英文界面；想法栏标题是固定中文，所以派发场景用快捷键而不是读想法行。
- **未做**：S7 统计事件；独立 subagent 的对抗式代码审查；`NORMA_VISUAL_PREVIEW` 按款式定格。
- **设置页不提供“仅脉冲”**（2026-10-24 按产品要求）：脉冲只作内部回落；旧版本若存过 `pulse`，读取时当作未设置。
