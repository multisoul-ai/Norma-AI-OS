# Onboarding 实施计划

> 日期：2026-06-28 · 关联 SPEC：[`docs/product-specs/2026-06-28-SPEC-onboarding.md`](../product-specs/2026-06-28-SPEC-onboarding.md)
> 执行方式：worktree `worktree-onboarding-feature` + subagent 逐 Task TDD（遵守 CLAUDE.md 默认开发模式）

## 0. 计划基线与设计修正（对 SPEC 的偏离，需 reader 知情）

侦察代码后对 SPEC 做两处修正，本计划以修正后的设计为准：

**修正 A — 不新增 `AppScreen::Onboarding` 变体。**
`on_new_terminal` / `on_new_claude_agent` / `on_new_codex_agent` / `on_reflow_layout` / `toggle_settings_panel` 等 6 处都有 `if !matches!(self.screen, AppScreen::Canvas)` 守卫（`app/mod.rs:163,197,211,252`、`settings/panel.rs:144`）。若新增 `Onboarding` 变体并在其下创建节点，会被守卫挡住。
→ 改为：在 `MultisoulApp` 加 `onboarding: OnboardingState`（仿 `settings_panel: SettingsPanelState`），用 `onboarding.is_active()` 条件挂载 overlay（仿 `mod.rs:474` 的 `if self.settings_panel.open`）。`screen` 仍只有 `Launchpad` / `Canvas`：Phase 1 在 `Launchpad` 上挂 overlay；「开始体验」切换 `screen = Canvas` 后，Phase 2 在 `Canvas` 上挂 overlay。守卫天然通过。

**修正 B — 元素锚点/聚光用「canvas→屏幕投影 + 静态偏移」，element bounds 查询作为 spike 闸门。**
全仓无 GPUI element bounds 查询先例。可行锚定路径：①toolbar 按钮用静态偏移（参考 `new_menu.rs:74`、chrome 顶部几何）；②canvas 节点用 `Viewport::canvas_to_screen` 投影节点逻辑坐标到屏幕坐标。聚光遮罩分两级降级（见 Task 7），spike 结果决定走哪级。

`OnboardingState` 持 `store: Option<SettingsStore>`（范式 A，参考 `shortcuts.rs:78,131`）以便 UI 反馈状态。

## 1. 任务依赖图

```
Task 0 (spike, 闸门) ─┐
Task 1 (状态机纯函数) ─┤
Task 2 (完成标志+触发) ┤── Task 6 (开始体验过渡+spawn 重构) ── Task 7 (画布段气泡+聚光+操作式检测) ──┐
Task 3 (快捷键默认绑定) ┘                                                                       │
Task 4 (通知权限模块) ── Task 5 (overlay 骨架+Launchpad 段+Hook 步) ───────────────────────────────┤
                                                                                                  → Task 8 (集成接线) → Task 9 (验证+约束)
```

Task 1/2/3/4 互相独立，可并行派发。Task 0 是 Task 7 的前置闸门。Task 5 依赖 Task 1/2/4。Task 7 依赖 Task 0/6/5/3。

## 2. Task 明细

每个 Task：TDD 先行（聚焦失败测试 → 实现），遵守 `docs/quality/test-requirements.md` 注释/断言规范。单文件 ≤500 行、无 `#[allow]`。

---

### Task 0 — GPUI overlay/锚点 spike（闸门，非 TDD）

**目标**：确定 Task 7 聚光遮罩走哪级，避免在不可行路径上投入。
**动作**：
- 在 worktree 写一个临时 throwaway 实验（不进 commit 主干，或作为 `#[cfg(test)]` 探针）：
  1. 能否在 gpui 0.2.2 取任意 element 的 layout bounds？（查 `gpui::Interactivity`/`CanvasContext`/`Element`/`request_layout`；若只有内部 API 不可达，记录「不可行」）
  2. 验证「canvas 节点逻辑坐标 → 屏幕坐标投影」`Viewport::canvas_to_screen` 可用，在 overlay `absolute().left/.top` 摆气泡定位到节点。
  3. 验证全屏半透明遮罩 + 中心面板裁切是否能做（`occlude()` + `absolute()` 叠层，参考 `panel.rs:161`）。
**产出**：在 plan 顶部（§0 下方）追加一节「Spike 结论」，明确选定的聚光级别（A 全屏聚光打洞 / B 全屏变暗 + 目标处高亮环 + 气泡 / C 仅居中气泡）。
**验证**：实验代码 `cargo check -p app` 通过；结论可被 Task 7 直接引用。
**若 spike 超时无定论**：默认采用 B 级（最稳，工作量可控），A 级作未来项。

---

### Task 1 — Onboarding 状态机纯函数（TDD）

**文件**：`crates/app/src/app/onboarding/state.rs`（新建），`mod.rs` 顶部 `mod onboarding;`。
**状态机**（参考 SPEC §5，去除 `CanvasIntro` 合并入过渡逻辑）：
```rust
pub(crate) enum OnboardingStep {
    Welcome, Permissions, Hooks,
    PracticeNewTerminal, PracticeArrowKeys, PracticeToggleLayout,
    PracticeCreateAgent, PracticeOpenSettings, ShowEsc, Done,
}
pub(crate) enum StepOutcome { Next, Skipped }
pub(crate) struct OnboardingState { active: bool, step: OnboardingStep, /* store, permission/hook status */ }
```
转移纯函数：`advance(state, outcome) -> OnboardingState`——`Next`/`Skipped` 都推进到下一步（skip 语义：跳过单步即去下一步，不回退）；`Done` 是终止态。
**TDD**（`onboarding/state_tests.rs` 或同文件 `#[cfg(test)]`）：
- Welcome --Next--> Permissions；Welcome --Skipped--> Permissions（跳过也推进）。
- 全链 Next：Welcome→…→ShowEsc→Done。
- 全链 Skip 到 Done 仍可达。
- `is_active()` 在 `Done` 后为 false。
- 权限/Hook 状态不影响转移顺序（注入 mock 状态，转移函数不读这些字段）。
- 每个 assert 带说明字符串；关键转移逐一断言，不用循环。
**验证**：`cargo test -p app onboarding::state`。

---

### Task 2 — 完成标志持久化 + 首启触发判定（TDD）

**键**：`onboarding.completed.v1`，值 `1`/未设。
**TDD 纯函数**（`onboarding/state.rs` 或 `onboarding/persistence.rs`）：
- `is_onboarding_completed(raw: Option<&str>) -> bool`：`Some("1")` => true，其余 false。
- 边界：`None`/`Some("0")`/`Some("true")` => false（只认 `"1"`，配注释说明为何只用 `"1"`）。
**接线**（`app/mod.rs::new`，line 133-137 前置变量区）：
```rust
use storage::AppStore;
let onboarding_done = AppStore::open_default().ok()
    .and_then(|s| s.settings_store().get_string("onboarding.completed.v1").ok().flatten())
    .map(|v| v == "1").unwrap_or(false);
```
`MultisoulApp` 加 `onboarding: OnboardingState`；`new` 里 `onboarding: OnboardingState::new(!onboarding_done, store),`（`store` 复用 shortcuts 已 open 的 `AppStore` → `settings_store()`）。
**持久化**：`OnboardingState::mark_completed(&mut self)` → `if let Some(s)=&self.store { s.set_string("onboarding.completed.v1","1") }`，置 `active=false`。
**TDD**：
- `is_onboarding_completed` 纯函数正负断言（`Some("1")` true 多组负样本 false）。
- 注入 mock store（或 `store: None`）的 `mark_completed` 不 panic、置 active=false。
**验证**：`cargo test -p app`。

---

### Task 3 — 快捷键默认绑定换绑（TDD，独立可并行）

**文件**：`crates/app/src/shortcuts.rs`。
**改动**：
- line 256：`NewTerminal` `default_keystroke: Some("cmd-n")` → `Some("cmd-1")`。
- line 280：`NewClaudeAgent` `default_keystroke: None` → `Some("cmd-2")`。
- line 288：`NewCodexAgent` `default_keystroke: None` → `Some("cmd-3")`。
- line 280/288 若 `editable: false`，改 `true`（让用户可在 Settings 录制/恢复，遵守 AGENTS.md §2）。实现前核实该字段当前值。
- 核实 `⌘1/⌘2/⌘3` 未与 GPUI/系统窗口切换冲突（gpui 0.2.2 无内置占用即可）。
**TDD**（`shortcuts.rs` 现有测试风格或新增）：断言 `default_shortcut_entries()` 中：
- 正断言：`NewTerminal`.default == `cmd-1`；`NewClaudeAgent`.default == `cmd-2`；`NewCodexAgent`.default == `cmd-3`；三者 `editable==true`。
- 负断言：默认条目集合里不再有 `cmd-n` 绑定到 `NewTerminal`；`cmd-1/cmd-2/cmd-3` 不与其它命令重复（无键位碰撞）。
- 每个 assert 带说明。
**验证**：`cargo test -p app`；手动 GPUI 测试（Task 9）：`⌘1/⌘2/⌘3` 创建对应节点、`⌘N` 不再建终端。

---

### Task 4 — 通知权限模块（TDD，独立可并行）

**文件**：`crates/voice_sherpa/src/permissions.rs`（已有麦克风逻辑，旁加通知）或 `crates/app/src/onboarding/notifications.rs`。倾向放 `voice_sherpa`（已是有 macOS 权限的 crate）。
**API**：`pub fn request_notification_permission() -> NotificationPermission`；`pub enum NotificationPermission { Granted, Denied, Unavailable }`。
**实现**：`objc2` + `UNUserNotificationCenter::requestAuthorizationWithOptions(alert|badge|sound)`（实现期核实 objc2 crate 覆盖度，仿现有麦克风 `AVCaptureDevice` 绑定方式）。
**未签名降级**：用纯函数 `availability(code_signature: SignatureKind) -> bool` 或检测 `current_exe` 未签名时直接返回 `Unavailable`（不调系统 API）。**降级判定必须是纯函数可单测**。
**TDD**：
- 降级纯函数：未签名 → `Unavailable`；签名 → 走请求（mock）。
- 边界：请求抛错 → `Denied`，不 panic。
**验证**：`cargo test -p voice_sherpa`（或所在 crate）。

---

### Task 5 — Overlay 骨架 + Launchpad 段（Welcome/Permissions/Hooks）（TDD，依赖 Task 1/2/4）

**文件**：`crates/app/src/app/onboarding/overlay.rs`。
**渲染**：仿 `settings/panel.rs:161` 的 overlay 模式——`absolute().size_full()` 半透明遮罩（可选，Launchpad 段可不强遮罩，参考 `new_menu.rs` 无遮罩 pill）+ 居中气泡卡片（标题/正文/主按钮/「跳过」）。仅在 `onboarding.is_active()` 时渲染；Phase 1 步骤（Welcome/Permissions/Hooks）气泡不依赖画布元素，居中或锚定 Launchpad 区。
**Phase 1 各步**：
- Welcome：欢迎文案 + 「下一步」。
- Permissions：两个按钮「允许麦克风」→ `voice_sherpa::request_microphone_permission()`；「允许通知」→ `request_notification_permission()`；显示状态绿/灰；麦克风被拒不阻塞；通知 `Unavailable` 显示「需签名版本」可跳过。
- Hooks：检测 `agent_hooks_installed(Claude/All, &default_install_paths())`（`norma::agent_hook`）；未装则两个按钮「Install Claude hooks」/「Install Codex hooks」→ 调 `install_agent_hooks(HookTarget::Claude/Codex)`（同步函数，参考 `settings/hooks.rs:79-90`），成功后状态转绿；已装直接显示已装。提示将写 `~/.claude/settings.json`。
- 「下一步」=「开始体验」→ 触发 Task 6 过渡。
**GPUI 测试**（参考现有 chrome 测试风格）：断言 overlay 仅在 `is_active()` 时渲染（inactive 时不产出气泡节点）；active 时产出当前 step 对应气泡。
**验证**：`cargo test -p app`；`cargo check -p app`。

---

### Task 6 — 「开始体验」过渡 + spawn 重构（TDD，依赖 Task 1/2/5）

**目标**：从 Launchpad 段末「开始体验」→ 自动建一个示例终端节点 + 切到 Canvas。
**重构**（`app/mod.rs`）：把 `on_new_terminal`（line 162-189）的 line 169-188 主体抽成 `fn spawn_terminal_in_visible_area(&mut self, window, cx)`（无 screen 守卫，`pub(crate)`）；`on_new_terminal` 改为「守卫 + 调 helper」。导出 `spawn_agent_card`（line 219）为 `pub(crate)` 供 onboarding 复用。
**过渡动作**（onboarding「开始体验」按钮 handler）：
1. `self.screen = AppScreen::Canvas;`
2. `self.spawn_terminal_in_visible_area(window, cx);`（建示例终端节点，shell `/bin/zsh`，沿用现有 shell 选择，示例内容见未决）
3. `onboarding.advance(Next)` → 进入 `PracticeNewTerminal`。
4. `cx.notify()`。
**TDD**：抽出的 helper 行为与原 `on_new_terminal` 在 `screen==Canvas` 时等价（节点被 spawn、reflow 触发）——用现有 on_new_terminal 测试改造断言「守卫拦截」+「helper 成功」两个分支。过渡函数：调用后 `screen==Canvas` 且 `step==PracticeNewTerminal`。
**验证**：`cargo test -p app`；现有 terminal 创建测试不回归。

---

### Task 7 — 画布段气泡 + 聚光 + 操作式检测（依赖 Task 0 spike 结论 / 3 / 6）

按 §0 Spike 结论选定聚光级别落地。
**操作式检测**（不直接拦截原始键事件）：
- 订阅 action：`⌘1`(NewTerminal)、`⌘M`(ToggleMasterLayout)、`⌘2/⌘3`(CreateAgent)、`⌘,`(ToggleSettings) 触发即 `advance(Next)`。复用现有 action 派发；确保 Onboarding overlay 不截断画布 action 路由（`stop_propagation` 只挡点击滚动，不挡键盘 action）。
- 方向键旁路：方向键不是 action（硬编码，见 SPEC §5）。在 `on_key_down`（`app/keyboard.rs:236-258`）路由到 `NavigateSelection`→`SelectNode` 成功时，**仅当 onboarding 在 `PracticeArrowKeys` 步**调用 `advance(Next)`。挂载/摘除由 `onboarding.step` 守卫，无常驻拦截。
- `Esc`（ShowEsc 步）：纯静态展示，不订阅、不检测；「下一步」手动推进。
**气泡锚定**：toolbar 按钮用静态偏移（参考 chrome 顶部几何、`new_menu.rs:74`）；canvas 节点（方向键步骤选中变化）用 `Viewport::canvas_to_screen` 投影到屏幕坐标摆气泡。
**聚光遮罩**（按 spike 级别）：A 级=全屏遮罩 + 目标处打洞；B 级=全屏变暗 + 目标处高亮环 + 气泡指向；C 级=仅居中气泡。
**操作式反馈**：按对自动推进；按错无惩罚静默等待。
**GPUI 测试**：当前 step 渲染对应气泡；操作式触发后 step 改变（可注入「已触发」状态测转移）。
**验证**：`cargo test -p app`；`cargo check -p app`；手动 GPUI 测试（Task 9）逐步按过 5 个操作式键。

---

### Task 8 — 集成接线 + 完成判定（依赖 Task 5/6/7）

**接线**：
- `render`（`app/mod.rs:461` match）的 `Launchpad` 与 `Canvas` 两支末尾，仿 `if self.settings_panel.open`（line 474）加 `if self.onboarding.is_active() { root = root.child(self.render_onboarding_overlay(window, cx)); }`。
- `ShowEsc → Done` 时或任意步「跳过全部」→ `onboarding.mark_completed()`（写 `onboarding.completed.v1=1`，置 inactive）。
- 中途退出/关窗：**不**调 `mark_completed`（符合「看完即完成」）。复用 main.rs:51 的 `on_window_closed`，无需改。
- 完成后保留示例画布（`screen` 保持 `Canvas`，不回 Launchpad）。
**TDD**：
- 状态机：`ShowEsc --Next--> Done` 且 `is_active()` 转 false。
- 「跳过全部」从首步到达 `Done` + `mark_completed`。
- 触发判定集成（Task 2 的 store 模拟）：标志未设 → `is_active()` true；已设 → false。
**验证**：`cargo test -p app`。

---

### Task 9 — 验证 + 机械化约束（收尾）

- `cargo test --workspace`
- `cargo check -p app`、`cargo build -p app --release`
- `bash scripts/check-max-file-lines.sh`（onboarding 模块拆分确保 ≤500）
- `bash scripts/check-no-allow.sh`
- `bash scripts/check-agent-guide-lines.sh`（若改了 AGENTS/CLAUDE）
- `bash scripts/check-storage-boundaries.sh`
- `cargo tree -p canvas -p terminal -p runtime | grep -i gpui`（应无输出，确认未污染域层）
- GPUI 实跑验证（`cargo run -p app`）：全新 db（删 `~/Library/Application Support/Multisoul/app.sqlite`）首启触发引导 → 走完全程 → 二启不触发；中途 ⌘Q 退出再启 → 仍触发。
- 快捷键实跑：`⌘1/⌘2/⌘3` 各建节点、`⌘N` 不再建终端、方向键切节点（仅 CanvasSelect 模式）。
- 通知降级实跑（未签名 build）：通知步显示「需签名版本」可跳过。
- 加载 `verification-before-completion` skill 后再声明完成。

## 3. 实施期未决（不阻塞开工，实现时细化）

- 示例终端节点内容：`/bin/zsh` 空态 vs 预制欢迎命令（倾向空态，避免 PTY 写入副作用）。
- 聚光洞形状/动画（依赖 Task 0 spike 级别）。
- `NewClaudeAgent`/`NewCodexAgent` 当前 `editable` 字段值需核实（Task 3）。

## 4. 风险与回退

| 风险 | 回退 |
|---|---|
| Task 0 spike 证明 element bounds 不可行 | 预期内，落 B/C 级降级 |
| `objc2` 通知授权覆盖度不足 | 该步降级为纯状态展示（不强求授权框），不阻塞 |
| onboarding overlay 截断画布 action 路由 | 改 overlay 仅 `stop_propagation` 鼠标/滚轮，不拦键盘 action |
| 方向键旁路检测影响常驻键盘路由 | 严格 `onboarding.step == PracticeArrowKeys` 守卫，完成即不影响 |
| 抽取 spawn helper 改动 on_new_terminal 行为 | TDD 守卫/helper 双分支测试 + 现有 terminal 测试回归 |
