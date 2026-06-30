# Onboarding 新手引导 SPEC

> 日期：2026-06-28 · 状态：已确认，待实施
> 来源：采访式需求梳理（interview-spec）+ 现状代码侦察（无 onboarding 现状）

## 1. 背景与目标

Multisoul Desktop 当前**无任何新手引导**：首启直接进 Launchpad，麦克风权限要到用语音才请求，Agent Hooks 要用户自己进 Settings → Hooks 手动 Install，方向键切换节点 / NewClaudeAgent 等快捷键默认未绑定或未注册。新手不知道这些能力存在。

**目标**：首启自动触发一次性新手引导，以**画布内浮层气泡**形式，分两阶段（Launchpad → 示例画布）带用户完成：①麦克风+通知权限开通 ②Agent Hooks 一键安装 ③常用快捷键操作式练习。完成后写 `onboarding.completed.v1` 标志，之后不再自动触发。

## 2. 范围

### 2.1 In Scope
- 首启自动触发判定（`onboarding.completed.v1` 未设 → 触发）。
- **权限开通**：麦克风（复用现有 `voice_sherpa::request_microphone_permission`）+ 通知权限（**新增授权代码**，气泡内点「允许提醒」触发）。
- **Agent Hooks 初始化**：引导气泡内「一键安装」按钮，复用 `crates/app/src/settings/hooks.rs` 的 `install_agent_hooks`（写 `~/.claude/settings.json` 与 Codex hooks 配置）；安装前检查 `agent_hooks_installed` 与 bridge binary 可达性。
- **快捷键引导（操作式）**：覆盖 `⌘1` 新建终端、`⌘M` 布局切换、`⌘,` 打开设置、方向键切换节点、创建 agent（Claude=`⌘2`/Codex=`⌘3`）；`Esc` 退出焦点仅**静态展示**。
- **顺手补全快捷键**：为三类创建统一数字键系列——`NewTerminal` 由 `⌘N` **换绑**为 `⌘1`，`NewClaudeAgent`=`⌘2`，`NewCodexAgent`=`⌘3`。方向键切换节点能力**已存在且硬编码**，本期不升级为可配置 action（理由见 §5）。
- 两阶段衔接：Launchpad 阶段末「开始体验」→ 自动创建带一个示例终端节点的示例画布 → 画布阶段气泡指向真实按钮。
- **完成后保留示例画布**（不删、不回 Launchpad），用户可立即上手。
- 完成标志持久化（复用 `settings_store` kv）。
- 逐步「跳过」语义：每步可「跳过」，看完即标记完成。

### 2.2 Out of Scope
- 引导重置入口（Settings 内「重看新手引导」）——未来再加，本期不做。
- 模态全屏向导、独立 Onboarding 截图页（已选定浮层气泡方案）。
- 自动静默安装 Agent Hooks（必须用户在气泡内点「一键安装」确认，不静默改外部配置）。
- 权限的「去系统设置深链」自动跳转（macOS 无法深链到 TCC 开关；仅展示状态 + 提示路径，详见 §8）。
- Accessibility / 输入监控 / 全盘访问权限（本期不申请，仅麦克风+通知）。

## 3. 用户与使用场景

1. 首启 → Launchpad 出现「欢迎」气泡 → 「下一步」。
2. 权限气泡：点「允许麦克风」→ 系统弹授权；点「允许提醒」→ 系统弹通知授权。
3. Hooks 气泡：检测到未安装 → 「一键安装 Claude/Codex hooks」→ 成功后状态转绿。
4. 「开始体验」→ 自动创建示例画布（含一个终端节点）。
5. 画布阶段：操作式气泡依次要求按 `⌘N`（出现终端节点）、方向键（节点选中变化）、`⌘M`（布局变化）、创建 agent（出现 agent 节点）、`⌘,`（设置面板打开）；`Esc` 步骤静态展示。
6. 任意步可「跳过」；走到末步或「跳过全部」→ 写完成标志 → 进正常 Launchpad/画布。
7. 二次启动：标志已设 → 不触发引导。

## 4. 业务流程与信息架构

- **触发判定**：`MultisoulApp::new`（`crates/app/src/app/mod.rs:96`）末尾，`workspace_state` 加载后，读 `onboarding.completed.v1`，未完成则置 `screen = Onboarding`（新增 `AppScreen::Onboarding` 变体，当前默认 `Launchpad`）。
- **阶段状态机**（新增 `OnboardingState`，纯状态可单测）：
  ```
  Launchpad 段：Welcome → Permissions(麦克风/通知) → Hooks → [Start]
                                                         │
  画布段：       NewTerminal → ArrowKeys → ToggleLayout → CreateAgent → Settings(⌘,) → Esc(静态) → Done
  ```
- **权限触发**：气泡内按钮点击 → 调权限函数（麦克风复用；通知新增）。
- **Hook 安装**：气泡内按钮 → 调 `install_agent_hooks(target)` → 刷新 `agent_hooks_installed` 状态 → 气泡状态更新。
- **示例画布创建**：Launchpad 段「开始体验」→ 调用与 `on_new_terminal` 等价的创建链路，落到一个新画布并 `screen = Canvas`，触发画布段第一个气泡。
- **完成**：写入 `onboarding.completed.v1 = "true"` → 落到正常 `Launchpad`（或保留当前画布，见 §10 未决）。

## 5. 数据模型与接口

**持久化**（复用 `settings_store` kv）：
- `onboarding.completed.v1` ∈ {`"true"`,`"false"`/未设}——完成标志。

**新增状态**（`app` 层，与 `AppScreen` 同层）：
```rust
pub(crate) enum OnboardingStep {
    Welcome,
    Permissions,            // 麦克风 + 通知（子状态）
    Hooks,                  // 安装/已安装
    CanvasIntro,            // 过渡：自动创建示例画布
    PracticeNewTerminal,    // 操作式：⌘1
    PracticeArrowKeys,      // 操作式：方向键切换节点（硬编码，不订阅 action）
    PracticeToggleLayout,   // 操作式：⌘M
    PracticeCreateAgent,    // 操作式：创建 agent 节点（⌘2 Claude / ⌘3 Codex）
    PracticeOpenSettings,   // 操作式：⌘,
    ShowEsc,                // 静态展示
    Done,
}
```

**快捷键补全**（键位已定，实现前与现有 9 条默认绑定去重 + 核实 `⌘1/⌘2/⌘3` 未与 GPUI/系统窗口切换冲突）：
- `NewTerminal` 由 `⌘N` **换绑**为 `⌘1`。
- `NewClaudeAgent` = `⌘2`，`NewCodexAgent` = `⌘3`（当前默认未设置，新增默认绑定）。
- 三者均在 `shortcuts.rs` 枚举内：`NewTerminal` 已在 `ShortcutCommandId` 仅改默认值；`NewClaudeAgent`/`NewCodexAgent` 已在枚举但默认未绑定，仅需补 `default_shortcut_entries()` 默认值。均需在 Settings → Keyboard 可录制/恢复（遵守 AGENTS.md §2）。

**方向键切换节点（不改硬编码）**：能力已端到端存在——`crates/app/src/app/keyboard.rs:236-258` 翻译方向键 → `canvas/navigate_card` 几何选邻居 → `route::NavigateSelection` → `Selection::select_only`。约束：
- 仅 `InputMode::CanvasSelect` 生效；节点聚焦态方向键进 PTY 不切节点（`runtime/route.rs:83`）。
- 带修饰键不触发（`keyboard.rs:109-111`）。
- **不可重绑**：`up/down/left/right` 是 `shortcuts.rs:458-463` 只读保留行为，非 `ShortcutCommandId`。引导文案不得声称「可在设置改键」；本期不升级为 action（列为 §10 未来项）。

**通知权限新增**（`app` 层或 `voice_sherpa` 旁新增 `permissions` 模块）：
- 调用 `UNUserNotificationCenter::requestAuthorizationWithOptions`（alert/badge/sound）。
- 未签名 app 降级：见 §8/§10。

**气泡锚点机制**（`app` 层渲染）：
- 为引导目标元素（「新建终端」按钮、布局按钮、设置入口、节点本身）打**锚点 id**，引导气泡在 overlay 层根据锚点 element bounds 计算箭头位置。**实现需 spike GPUI 0.2.2 取 element bounds 的可行 API**（见 §10 风险）。

## 6. 技术实现概览

**架构边界**：Onboarding 是纯 UI/编排逻辑，全留 `app` 层（新增 `app/onboarding/` 模块，含状态机 + 气泡渲染 + 锚点定位）。状态机 `OnboardingStep` 转移纯函数化以便单测；权限/hook/示例画布创建作为副作用调用方下沉的现成函数，不在域层新增依赖。完成标志借道 `settings_store`。**不下沉 domain crate。**

**模块拆分**（遵守 500 行单文件上限）：
- `app/onboarding/state.rs`：`OnboardingStep` + 转移纯函数（可单测）。
- `app/onboarding/overlay.rs`：气泡渲染 + 锚点定位。
- `app/onboarding/mod.rs`：编排 + 与权限/hook/画布创建的胶水。

**操作式键盘检测**：在 Onboarding 激活时，画布段订阅目标 action（`NewTerminal`/`ToggleMasterLayout`/`NewClaudeAgent`/`ToggleSettings`）的触发；触发即视为「按过」，推进下一步。**不直接拦截原始键事件**，避免与既有 Esc/ExitFocus 路由冲突——复用现有 action 分发。两步不走 action：`Esc` 步骤纯静态；方向键步骤——因方向键切换是 `on_key_down` 内联分支而非 action（见 §5），该步改用「在 `on_key_down` 捕获到 `NavigateSelection` 路由结果时推进」的旁路检测（仅 Onboarding 激活时挂载，完成即摘除）。

**约束**：`gpui=0.2.2` pinned；500 行单文件上限（拆分见上）；无 `#[allow(...)]`；TDD 先行（状态机转移优先单测）。

## 7. UI/UX 需求

- 气泡：圆角卡片 + 指向目标元素的箭头，含标题/正文/主按钮/「跳过」。
- Launchpad 段气泡不依赖画布元素，居中或锚定 Launchpad 区。
- 画布段气泡锚定到真实按钮；目标元素采用**全屏聚光遮罩**——全屏半透明遮罩，仅聚光目标元素，其余变暗（视觉引导最强；GPUI 0.2.2 overlay 可行性为强 spike 项，见 §10）。
- 权限/Hooks 步骤显示当前状态（未做/已授权/已安装），绿/灰图标。
- 操作式步骤：显示要按的键，按对后自动推进；按错无惩罚（静默等待）。
- 「跳过」单步；末步前有「跳过全部」。
- 视觉沿用 `crates/ui/src/theme.rs` 现有色板。

## 8. 状态、错误与边界情况

| 场景 | 行为 |
|---|---|
| 二次启动（标志已设） | 不触发引导 |
| 用户全程「跳过全部」 | 视为看过，写完成标志，不再触发 |
| 麦克风被拒 | 气泡状态转「未授权，可稍后在系统设置开启」，不阻塞下一步 |
| 通知权限未签名 app 请求失败 | 降级：状态显示「需签名版本支持」，不强求；不 panic + `tracing::warn!` |
| Hook 安装失败（bridge binary not found / 写入失败） | 气泡显示失败原因，可重试，不阻塞下一步 |
| Hook 已安装（二启或重看时） | 直接显示已安装，跳过安装动作 |
| 示例画布创建失败 | 回退到 Launchpad，画布段气泡不可用则跳过画布段 + warn |
| 操作式步骤用户长时间不按 | 不强制超时，等用户「跳过」或按键 |
| 目标锚点元素未渲染（如设置面板未开） | 气泡降级居中显示，不指向 |
| 退出/关窗中断引导 | **不写**完成标志，下次启动重新触发（符合「看完即完成」） |

## 9. 非功能性需求

- **性能**：气泡 overlay 仅在 Onboarding 激活时存在；锚点定位在 layout 后计算，不进 PTY drain / mouse move 热路径（遵守存储/渲染热路径禁令的同类约束）。
- **可测试性**：状态机转移纯函数化；权限/Hook 状态判定可注入 mock。
- **可维护性**：引导步骤序列集中在 `OnboardingStep` 枚举，新增步骤一处改动。
- **安全**：Hook 一键安装明确提示将写入 `~/.claude/settings.json` 外部配置；不静默改外部文件。

## 10. 风险、权衡与未决问题

- **风险（高）：未签名 app 通知权限受限**。`UNUserNotificationCenter` 需签名 bundle id + entitlement 才稳定弹授权框；当前打包未签名（Phase 6 之前）。降级策略：检测未签名时该步只展示状态、不强求授权框（已在 §8 落地）。彻底解决依赖 Phase 6 codesign。
- **风险（高）：GPUI 0.2.2 overlay + element bounds 未验证**。气泡「指向真实按钮」与「全屏聚光遮罩」都依赖：①拿到目标元素屏幕 bounds；②一个能覆盖全屏并裁切出聚光洞的 overlay 层。实现前需 spike：GPUI 0.2.2 `interactivity`/`Element`/layout 阶段 bounds 提取 + overlay 层级与裁切。若不可行，两级降级：先降为「仅高亮描边」，再降为「居中气泡 + 文字描述目标」。
- **风险（中）：操作式检测复用 action 分发** + 方向键旁路检测。非 action 的步骤（方向键）需在 `on_key_down` 旁路捕获，仅在 Onboarding 激活时挂载，完成即摘除，避免常驻拦截。需确认 Onboarding 激活时画布 action dispatch 路径不被 overlay 截断。
- **风险（低）：`⌘1/⌘2/⌘3` 与系统/GPUI 窗口切换习惯冲突**。实现前核实 GPUI 0.2.2 是否预留 `⌘1/⌘2/⌘3`，避免占用。
- **权衡**：分两阶段 Launchpad→示例画布 + 全屏聚光，编排与 overlay 复杂度最高，换来画布段能指向真实节点（产品价值高，已接受）。
- **未来项（本期不做）**：方向键切换节点升级为可配置 action（当前硬编码只读）；引导重置入口（Settings 内「重看新手引导」）。
- **未决（实现期细化）**：聚光遮罩的洞形状（矩形 vs 圆角）与动画；示例画布里示例终端节点的具体内容（shell `/bin/zsh` 还是带欢迎文本的预制命令）。

## 11. 验收标准与示例

**必须达成**：
- [ ] 首启（`onboarding.completed.v1` 未设）自动触发引导；二次启动不触发。
- [ ] Launchpad 段：欢迎 → 麦克风（可授权）→ 通知（可授权，未签名降级）→ Hooks 一键安装，四步可走通。
- [ ] 「开始体验」后自动创建示例画布并进入画布段。
- [ ] 画布段操作式：`⌘1`、方向键、`⌘M`、创建 agent、`⌘,` 各步「按对即推进」；`Esc` 静态展示。
- [ ] 默认快捷键：`NewTerminal`=⌘1（由 ⌘N 换绑）、`NewClaudeAgent`=⌘2、`NewCodexAgent`=⌘3，且与现有默认绑定无键位碰撞；三者在 Settings → Keyboard 可录制/恢复。
- [ ] 引导走完后保留示例画布（不删、不回 Launchpad）。
- [ ] 全屏聚光遮罩在画布段步骤正确聚焦目标元素（spike 通过后；未通过则按 §10 降级，相应验收放宽）。
- [ ] 每步可「跳过」；「跳过全部」/ 走到末步 → 写 `onboarding.completed.v1=true`。
- [ ] 中途退出不写完成标志，下次重启重新触发。
- [ ] 麦克风被拒 / 通知降级 / Hook 失败 / 示例画布失败：不 panic，不阻塞，可跳过继续。

**代表性用例**：
1. 全新用户首启 → 走完全程 → 关窗 → 二启进正常 Launchpad（核心路径）。
2. 用户全程「跳过全部」→ 二启不再触发。
3. 未签名 build 走到通知步 → 降级展示「需签名版本」→ 可跳过继续。

**测试范围**（TDD，遵守项目测试注释规范 `docs/quality/test-requirements.md`）：
- 单元层：`OnboardingStep` 转移状态机（含「跳过」分支、权限状态不影响转移顺序、完成判定纯函数）——纯函数化便于测试。
- 单元层：默认快捷键绑定集合含 `NewTerminal`=`⌘1`、`NewClaudeAgent`=`⌘2`、`NewCodexAgent`=`⌘3`，且 `⌘N` 不再绑定 `NewTerminal`、与现有默认绑定无键位碰撞（正断言存在 + 负断言无碰撞）。
- 单元层：完成标志编解码（`onboarding.completed.v1` 读写）。
- 集成：触发判定（标志未设→Onboarding；已设→不触发）。
- GPUI 层：气流 overlay 仅在 Onboarding 激活时渲染（参考现有 chrome 测试风格）。
