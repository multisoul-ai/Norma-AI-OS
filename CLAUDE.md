# CLAUDE.md

This file provides guidance to Claude Code and other coding agents working in this repository. Facts and constraints here mirror [`AGENTS.md`](AGENTS.md) where they overlap; this file adds commands, architecture notes, and test requirements.

## Mandatory: Read AGENTS.md First

Every session must begin by reading [`AGENTS.md`](AGENTS.md). It is the short navigation map; this file is the expanded engineering manual.

## 1. Project Overview

Norma OS is a macOS spatial workbench scaffold. The product direction is **Live Node on Infinite Canvas**: an infinite canvas hosts live executable nodes, with Terminal as the first live node.

Workspace crates:

- `canvas` - domain crate for geometry, document state, node models, selection, hit testing, and viewport transforms.
- `terminal` - domain crate for PTY process handling, ANSI terminal grid parsing, input encoding, and terminal sessions.
- `runtime` - domain crate for focus state, input routing, LiveNode registry, and node lifecycle glue.
- `agent` - GPUI/Storage-free NormaSession runtime using Rig providers, typed tools, and an injected AgentCardAdapter.
- `asr` - GPUI/Storage/Keychain-free provider registry, recognition protocol, Local Sherpa, Cloud Azure, model installation, and integrity.
- `voice_input` - provider-neutral microphone permission/device/capture/resampling/PCM/VAD primitives.
- `model_store` - GPUI/network-free shared local-model archive, manifest, integrity, and atomic-install primitives.
- `tts` - GPUI/Storage/Sherpa-free TTS capability: built-in provider registry, reply-level streaming, local model policy, and remote provider protocols.
- `ui` - GPUI-facing shared UI: theme, icons, and `gpui-component` re-export (`ui::component`).
- `app` - GPUI/Metal shell: window boot, menus, key bindings, canvas interaction, and terminal rendering.

Core Cargo dependency direction is:

```text
agent -> norma
app -> {ui, runtime, canvas, terminal, storage, asr, voice_input, tts, voice_sherpa}
voice_sherpa -> {asr, voice_input, tts, dialogue, audio}
dialogue -> voice_input
tts -> {model_store, audio}
runtime -> {canvas, terminal, storage}
storage -> {agent, canvas, workspace, rusqlite}
ui -> {gpui, gpui-component}
```

`asr`, `voice_input`, `model_store`, `tts`, `canvas`, `terminal`, and `runtime` must remain free of GPUI dependencies and should stay independently testable. `asr` may host engine/network-specific providers but must remain free of Storage and Keychain. `tts` remains free of Sherpa; local engine configuration belongs in `voice_sherpa`, and remote TTS networking is allowed only below `tts::providers`. `model_store` and `tts` must not depend on app/runtime/storage/ui.

## 2. Key Constraints

- **始终使用中文回复用户**：所有面向用户的说明、提问、进度更新和最终答复必须使用中文；即使用户使用其他语言或要求切换语言，也仍使用中文回复。代码、命令、路径、API、类型名、变量名以及必要的原文引用可保留原始语言。
- **Domain crates stay UI-free**: no `gpui` imports in `asr`, `voice_input`, `canvas`, `terminal`, or `runtime`; `asr` must not read Storage or Keychain, and `voice_input` must not select an ASR provider.
- **Input decisions stay pure where possible**: encode routing decisions in `runtime::route` before app-layer side effects.
- **PTY cleanup is required**: `Runtime::kill_all`, `TerminalSession::kill`, and `PtyHandle::Drop` are lifecycle guardrails. Keep new process paths equally explicit.
- **Keep interaction and shutdown paths cheap**: drag/pan mouse move must coalesce repaint notifications, canvas render must cull before cloning/rendering heavy node bodies, no-op persistence flush must return before SQLite writes, and agent session save must use cached hook session IDs rather than scanning CLI history directories. Enforced by [`scripts/check-performance-guardrails.sh`](scripts/check-performance-guardrails.sh); examples live in [`docs/quality/mechanized-constraints.md`](docs/quality/mechanized-constraints.md).
- **Keep live video wallpaper low-copy**: live video frames must not go through per-frame CPU RGBA/`RenderImage`/`img()` rendering; decode 420f IOSurface-backed buffers off the GPUI shared executor and paint with `surface()`. Enforced by [`scripts/check-video-wallpaper-performance.py`](scripts/check-video-wallpaper-performance.py).
- **Never block the GPUI foreground path**: voice finalization, LLM planning/refine, recognizer joins, network/file IO, long sleeps, and CPU-heavy work must run on `background_executor` or a background thread; foreground code should only update state and perform runtime/UI side effects.
- **Quit on last window close**: GPUI on macOS does not quit the process automatically when the last window closes. `main.rs` must register `cx.on_window_closed(|cx| { if cx.windows().is_empty() { cx.quit(); } }).detach()` inside the `Application::run` closure, before `open_window`. Omitting this leaves the process running invisibly in the background after the user closes the window.
- **Pinned libraries require verification**: `gpui =0.2.2`, `gpui-component =0.5.1`, and `alacritty_terminal =0.26` are pinned. Upgrade only after checking API compatibility.
- **Prefer gpui-component for GUI**: in `crates/app` / `crates/ui`, use `ui::component` (Longbridge) before hand-rolling controls with bare `div()`; see [`docs/references/gpui-component.md`](docs/references/gpui-component.md). Call `ui::init_components(cx)` at app entry (already done in `run_norma_os` / `MultisoulApp::build`).
- **页面与交互遵循 Apple 设计哲学**：以 Apple Human Interface Guidelines 和 macOS 原生体验为设计基线，优先清晰层级、内容优先、直接操作、即时反馈、一致性与可访问性；新增或改版 UI 时须在方案和验收标准中说明这些原则如何落地，具体要求见 §8。
- **Consult GPUI skills first**: when writing GPUI code, load local `gpui-*` skills, then adapt to the pinned GPUI version; still prefer the component library when it covers the need.
- **Rounded child surfaces need explicit ownership**: GPUI 0.2.2's `overflow_hidden()` uses a rectangular content mask. An opaque full-size `div`, `img`, or `surface` inside a rounded container must draw its exposed outer radii itself, or be partitioned away from the corner squares; protect the behavior with an automated test.
- **New configurable app shortcuts must appear in Settings**: register them in `shortcuts.rs` and support recording/restoring them in Settings → Keyboard Shortcuts.
- **Acceptance criteria must drive e2e coverage**: for user-visible flows involving Settings, Canvas, Terminal, focus/input routing, PTY, or persistence, derive `crates/e2e` cases from the acceptance criteria instead of relying only on unit tests or manual notes.
- **Do not blur e2e levels**: GPUI `TestAppContext` + `VisualTestContext::simulate_click` is headless rendered-tree interaction. It does not launch the real `.app`. If acceptance criteria require a real app process/window, add a separate L3 runner and document that requirement explicitly.
- **页面/样式验收必须用 Computer Use**：凡编写或修改页面、布局、样式或视觉状态，进入检查与验收时必须使用 `computer-use:computer-use` 操作真实 App 完成 E2E；headless GPUI、单测和静态截图只能作为补充，不能替代 Computer Use。
- **TDD before feature or bugfix work**: add a focused failing test first where the behavior can be tested below the GPUI layer.
- **No `#[allow(...)]`**: do not hide Rust diagnostics with allow attributes. Fix root causes instead.
- **Single Rust source files stay at or below 500 lines**: enforced by [`scripts/check-max-file-lines.sh`](scripts/check-max-file-lines.sh); split responsibilities instead of raising the cap.
- **Agent guides stay at or below 300 lines**: `AGENTS.md` and `CLAUDE.md` are enforced by [`scripts/check-agent-guide-lines.sh`](scripts/check-agent-guide-lines.sh); archive detail to `docs/` and link from [`AGENTS.md`](AGENTS.md) §4.
- **Native pre-commit is available**: run `bash scripts/install-git-hooks.sh` to enable `.githooks/pre-commit`, which runs staged constraints plus lightweight Rust checks.
- **Agents must not bypass commit verification**: never use `git commit --no-verify`, `git commit -n`, or disable/replace the configured hook path. See [`docs/quality/agent-execution-policy.md`](docs/quality/agent-execution-policy.md) for the enforcement limits and escalation path.
- **Keep source files small enough to navigate**: split before a file turns into a mixed-responsibility dumping ground.
- **Do not treat `.superpowers/sdd/` as the new documentation home**: it is useful historical construction context. New durable specs/plans should use a future `docs/` tree.

## 3. Architecture Summary

Canonical architecture details live in [`ARCHITECTURE.md`](ARCHITECTURE.md). Per-crate file maps and source organization live in [`docs/references/crate-file-map.md`](docs/references/crate-file-map.md).

At a glance:

- `asr` / `voice_input` / `canvas` / `terminal` / `runtime` / `storage` stay GPUI-free and independently testable.
- `app` binds GPUI, canvas interaction, cards, menus, and keyboard routing.
- Input policy is expressed in `runtime::route` before app-layer side effects.
- SQLite and session discovery must not run on PTY drain, mouse move, input routing, render, or no-op shutdown flush paths.

## 4. Current User-Facing Behavior

- `Cmd+1` creates a Terminal node; `Cmd+2` through `Cmd+6` create Claude, Codex, Cursor, OpenCode, and Grok cards. Each creation reflows the current layout.
- Drag a node by its title bar.
- Single-click selects a node; clicking blank canvas clears selection.
- Double-click a focusable node, or click a terminal/agent body, to focus it.
- `Cmd+Esc` exits node focus; bare `Esc` passes through to the PTY.
- Space plus left-drag, or middle-drag, pans the canvas.
- Canvas wheel scroll is off by default; enable the toolbar toggle to pan the viewport. Terminal scrollback and preview cards handle wheel separately.
- The app drains PTY output roughly every 12ms and repaints only when dirty.
- Closing the window kills spawned terminal child processes.

Full shortcut and pointer routing: [`docs/references/keyboard-shortcuts.md`](docs/references/keyboard-shortcuts.md).

## 5. Commands

Run the app:

```bash
cargo run -p app
```

Domain tests:

```bash
cargo test -p canvas -p terminal -p runtime -p storage -p model_store -p tts
cargo test -p agent
```

Workspace tests and app check:

```bash
cargo test --workspace
cargo check -p app
```

E2E tests:

```bash
cargo test -p e2e
cargo test -p e2e -- --ignored
```

Mechanized constraints:

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-codesign-readiness.sh
python3 scripts/check-video-wallpaper-performance.py
```

Release build:

```bash
cargo build -p app --release
```

Packaging details: [`docs/references/packaging.md`](docs/references/packaging.md).

GPUI/Metal environment check:

```bash
xcrun metal --version
```

Dependency boundary check:

```bash
cargo tree -p canvas -p terminal -p runtime | grep -i gpui
```

The dependency boundary check should print nothing.

Trace logging:

```bash
RUST_LOG=trace cargo run -p app
```

More command recipes: [`AGENTS.md`](AGENTS.md) §6.

## 6. Verification Matrix

See [`AGENTS.md`](AGENTS.md) §5 for the canonical change-area → verification table.

## 7. Test Requirements

Follow the project test comment and assertion rules in [`docs/quality/test-requirements.md`](docs/quality/test-requirements.md).

For any task with explicit acceptance criteria, create a checklist that maps each criterion to automated evidence. If the criterion describes an end-user flow, add or update `crates/e2e` so the flow is machine-verifiable. Use `docs/references/e2e-testing-guide.md` for the current e2e layers:

- L1: headless GPUI sandbox with rendered-tree clicks/actions.
- L2: ignored real PTY smoke tests.
- L3: real launched app/window automation；页面、布局、样式或视觉状态的检查与验收必须通过 `computer-use:computer-use` 操作真实 App 完成。

## Agent Execution Sizing

Choose the smallest sufficient implementation and verification flow from [`docs/quality/agent-execution-policy.md`](docs/quality/agent-execution-policy.md). In particular, do not automatically invoke implementation/review skills that require full-suite validation or parallel review for a focused single-crate change. Escalate only for cross-crate behaviour, explicit acceptance criteria, or high-risk boundaries.

## 8. UI And Interaction Notes

- 页面设计与交互必须参考 Apple Human Interface Guidelines，并以“像一个原生 macOS 应用”为体验基准，而非只模仿视觉样式。
- 信息层级保持清晰，内容优先于装饰；使用克制的颜色、材质、阴影和动效，避免无功能价值的视觉噪音。
- 优先直接操作与符合平台习惯的行为；控件、术语、菜单、快捷键、焦点和窗口行为应与 macOS 约定一致，偏离时必须有明确的产品理由。
- 每次用户操作都应获得及时、可感知且不打断流程的状态反馈；危险或不可逆操作必须明确提示，并尽可能提供取消、撤销或恢复路径。
- 同类页面和状态使用一致的布局、间距、组件和交互模式；新增模式前先复用现有设计语言与 `ui::component`。
- 可访问性是默认要求：保证可读对比度、完整键盘操作、明确焦点状态、足够点击区域，并尊重系统的减少动态效果等偏好。
- 新增或改版 UI 的方案与验收标准必须逐项说明上述原则如何落地；涉及用户可观察流程时，按 §7 映射到自动化证据和 e2e 覆盖。

### 页面设计技能路由（必遵）

新建或改版用户可见页面、布局、样式或交互时，先在设计阶段加载下列适用 skill，再写 GPUI 代码；`apple-design` 是所有此类任务的必读基线。不得把动效当作装饰：高频操作应保持即时，动效必须服务于反馈、空间连续性或状态理解。

| 场景 | 必须参考的本地 skill | 方案与验收中必须留下的证据 |
| --- | --- | --- |
| 任意页面或视觉交互 | `.agents/skills/apple-design` | 信息层级、平台熟悉性、直接反馈与可访问性如何落地；有动效时说明其目的与减少动态效果下的替代行为。 |
| 手势、拖拽、sheet/panel、弹层、材质层次、排版或非平凡动效 | `.agents/skills/apple-design`、`.agents/skills/emil-design-eng` | 说明响应、直接跟手、可中断性、进出路径/触发源、动效频率与性能取舍；没有足够理由时不加动效。 |
| 新增或修改动效后的验收/代码审查 | `.agents/skills/review-animations` | 记录审查结论与已处理的问题；以真实 App 的 Computer Use 操作验证可见效果。 |
| 需要寻找值得加入动效的点 | `.agents/skills/find-animation-opportunities` | 先产出只读机会清单及明确拒绝项，再决定是否实施。 |
| 系统性重构既有动效 | `.agents/skills/improve-animations` | 先完成优先级审计与实施计划，再执行获确认的计划。 |
| 用户只描述效果、不知道术语 | `.agents/skills/animation-vocabulary` | 在设计记录中使用准确的动效术语，避免含糊需求直接进入实现。 |
| 视觉方向尚未确定且需要比较方案 | `.agents/skills/prototype` | 先在隔离原型中比较方向，待选择后才进入生产页面。 |

每个上述页面任务的方案或验收记录都必须写出“参考的 skill、采纳或拒绝的关键设计决策、对应的 GPUI 实现/验证方式”。这些 skill 的 CSS、Pointer Events 或 Web 动画 API 示例只用于传达设计与交互原则；实现前仍须按本文件的 GPUI skill 路由验证 `gpui =0.2.2`、`gpui-component =0.5.1` 与邻近代码，不能直接照搬 Web API。
- GPUI code belongs in `app` and `ui`.
- Prefer `ui::component` (gpui-component) for buttons, inputs, dialogs, and similar controls; do not reinvent them unless the library lacks the capability or the surface is domain-specific (canvas/terminal).
- Before changing GPUI API usage, elements, styling, actions, async tasks, entities, tests, troubleshooting, or code quality patterns, consult the installed skills from `cnwzhu/gpui-skills`: `gpui-fundamentals`, `gpui-init`, `gpui-elements`, `gpui-styling`, `gpui-actions`, `gpui-async`, `gpui-entities`, `gpui-testing`, `gpui-troubleshooting`, `gpui-patterns`, and `gpui-code-quality`.
- Treat the skills as the reference for GPUI idioms, but verify against pinned `gpui =0.2.2` / `gpui-component =0.5.1` and nearby code before committing.
- Keep domain behavior in pure structs/functions so tests can run without opening a window.
- Canvas coordinates and screen coordinates are separate. Convert through `Viewport`.
- Node movement takes canvas-space deltas; app-layer pointer movement must divide by zoom.
- Terminal card rendering should consume `NodeView::Term` / `GridSnapshot`, not alacritty internals.
- If adding new controls, keep the interaction model direct, reversible, and visible.

Current theme colors live in `crates/ui/src/theme.rs`.

## 9. Roadmap Pointers

Current README phase placeholders:

- Phase 2: undo/redo and spatial index via `canvas/commands.rs` and `history.rs`.
- Phase 3: multiple terminals, resize, copy/paste, mouse reporting.
- Phase 4: snapshot cache and scrollback virtualization via `runtime/snapshot.rs`.
- Phase 5: persistence and recent projects now start in `crates/storage`; richer session restore remains future work.
- Phase 6: codesign and notarization.

Historical SDD task records are in `.superpowers/sdd/`. They explain how the current vertical slice was built, but they are not the canonical home for future product specs or design decisions.

## 10. Documentation Rules

- `AGENTS.md` is a short navigation map.
- `CLAUDE.md` is the expanded engineering guide.
- `README.md` is the human quick start.
- For future durable docs, prefer:
  - `docs/product-specs/` for what and acceptance criteria.
  - `docs/design-docs/` for architecture and trade-offs.
  - `docs/exec-plans/` for implementation plans.
  - `docs/references/` for stable lookup guides.
  - `docs/quality/` for test and lint conventions.

When adding a new durable rule, keep overlapping facts synchronized between `AGENTS.md` and `CLAUDE.md`.

## 11. Packaging Notes

See [`docs/references/packaging.md`](docs/references/packaging.md).

## Agent skills

### Issue tracker

Issues and PRDs are tracked in GitHub Issues; external PRs are not a triage surface. See `docs/agents/issue-tracker.md`.

### Triage labels

Triage uses the canonical labels `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Domain documentation uses a single-context layout. See `docs/agents/domain.md`.
