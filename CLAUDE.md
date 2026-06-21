# CLAUDE.md

This file provides guidance to Claude Code and other coding agents working in this repository. Facts and constraints here mirror [`AGENTS.md`](AGENTS.md) where they overlap; this file adds commands, architecture notes, and test requirements.

## Mandatory: Read AGENTS.md First

Every session must begin by reading [`AGENTS.md`](AGENTS.md). It is the short navigation map; this file is the expanded engineering manual.

## 1. Project Overview

Multisoul Desktop is a macOS spatial workbench scaffold. The product direction is **Live Node on Infinite Canvas**: an infinite canvas hosts live executable nodes, with Terminal as the first live node.

Workspace crates:

- `canvas` - domain crate for geometry, document state, node models, selection, hit testing, and viewport transforms.
- `terminal` - domain crate for PTY process handling, ANSI terminal grid parsing, input encoding, and terminal sessions.
- `runtime` - domain crate for focus state, input routing, LiveNode registry, and node lifecycle glue.
- `ui` - GPUI-facing shared UI primitives, currently theme only.
- `app` - GPUI/Metal shell: window boot, menus, key bindings, canvas interaction, and terminal rendering.

Core Cargo dependency direction is:

```text
app -> {ui, runtime, canvas, terminal}
runtime -> {canvas, terminal}
ui -> gpui
```

`canvas`, `terminal`, and `runtime` must remain free of GPUI dependencies and should stay independently testable.

## 2. Key Constraints

- **Domain crates stay UI-free**: no `gpui` imports in `canvas`, `terminal`, or `runtime`.
- **Input decisions stay pure where possible**: encode routing decisions in `runtime::route` before app-layer side effects.
- **PTY cleanup is required**: `Runtime::kill_all`, `TerminalSession::kill`, and `PtyHandle::Drop` are lifecycle guardrails. Keep new process paths equally explicit.
- **Pinned libraries require verification**: `gpui =0.2.2` and `alacritty_terminal =0.26` are pinned in the workspace. Upgrade only after checking API compatibility.
- **Consult GPUI skills first**: when writing GPUI code in `crates/app` or `crates/ui`, load the relevant local `gpui-*` skill before inventing a pattern, then adapt it to this repository's pinned GPUI version and existing style.
- **TDD before feature or bugfix work**: add a focused failing test first where the behavior can be tested below the GPUI layer.
- **No `#[allow(...)]`**: do not hide Rust diagnostics with allow attributes. Fix root causes instead.
- **Single Rust source files stay at or below 500 lines**: enforced by [`scripts/check-max-file-lines.sh`](scripts/check-max-file-lines.sh); split responsibilities instead of raising the cap.
- **Native pre-commit is available**: run `bash scripts/install-git-hooks.sh` to enable `.githooks/pre-commit`, which runs staged constraints plus lightweight Rust checks.
- **Keep source files small enough to navigate**: split before a file turns into a mixed-responsibility dumping ground.
- **Do not treat `.superpowers/sdd/` as the new documentation home**: it is useful historical construction context. New durable specs/plans should use a future `docs/` tree.

## 3. Architecture Summary

Canonical architecture details live in [`ARCHITECTURE.md`](ARCHITECTURE.md). This section is the working summary.

### `canvas`

Purpose: model the infinite canvas without UI dependencies.

Important files:

- `geometry.rs` - `Vec2`, `CanvasRect`, `Viewport`, and `Transform2D`; convention is `screen = canvas * zoom + pan`.
- `document.rs` - `Document`, node storage, viewport, monotonic node ids, z-index allocation, node movement.
- `node.rs` - `CanvasNode`, `NodeKind`/`CanvasNodeKind`, `NodeType`, `RectNode`, `TermSpec`, `AgentSpec`.
- `selection.rs` - stable ordered selection set based on `BTreeSet`.
- `hit_test.rs` - canvas-space hit testing that returns the highest z-index node.
- `commands.rs` and `history.rs` - Phase 2 placeholders for undo/redo.

### `terminal`

Purpose: turn a real PTY into a neutral grid snapshot that UI can render.

Important files:

- `pty.rs` - `portable-pty` wrapper, read thread, output channel, resize/write/kill.
- `grid.rs` - `alacritty_terminal` wrapper producing `GridSnapshot` with neutral `Cell` values.
- `input.rs` - `KeyInput` to PTY byte encoding, including Ctrl, Alt, Enter, arrows.
- `session.rs` - `TerminalSession`, drain loop, status, resize, snapshots, cleanup.
- `message.rs` - PTY channel events.

### `runtime`

Purpose: assemble canvas documents and LiveNode runtimes while keeping policy testable.

Important files:

- `live.rs` - `LiveNode`, `Caps`, `NodeInput`, `NodeResize`, and `NodeView`.
- `nodes/term.rs` - `TermLive`, the terminal-backed live node wrapper.
- `nodes/agent.rs` - Agent command factory/wrapper for terminal-backed live nodes.
- `focus.rs` - `CanvasSelect` and `NodeFocused(node_id)`.
- `route.rs` - pure input routing from mode/event to route action.
- `runtime.rs` - document, selection, focus, `NodeId -> LiveNode` registry, spawn/drain/kill.
- `snapshot.rs` - Phase 4 placeholder for snapshot caching and scrollback virtualization.

### `ui` and `app`

Purpose: GPUI shell and rendering.

Important files:

- `crates/ui/src/theme.rs` - dark theme colors as `0xRRGGBB`.
- `crates/app/src/main.rs` - tracing, GPUI application boot, `Cmd+N`, `Esc`, menu.
- `crates/app/src/app.rs` - `MultisoulApp`, background drain loop, key handling, lifecycle cleanup.
- `crates/app/src/canvas_view.rs` - pan, zoom, node drag, click/double-click, viewport culling, node chrome.
- `crates/app/src/cards/mod.rs` - `Card` trait and static `NodeType -> Card` registry.
- `crates/app/src/cards/chrome.rs` - shared card chrome helpers.
- `crates/app/src/cards/term/` - terminal card renderer, terminal view, and grid painter.
- `crates/app/src/cards/agent/` - agent card renderer.

## 4. Current User-Facing Behavior

- `Cmd+N` creates a terminal node, then reflows all nodes into a row-major tiled grid with at most three columns.
- Drag a node by its title bar.
- Single-click selects a node; clicking blank canvas clears selection.
- Double-click a focusable node, or click a terminal/agent body, to focus it.
- `Esc` exits node focus.
- Space plus left-drag, or middle-drag, pans the canvas.
- Mouse wheel zooms around the cursor.
- The app drains PTY output roughly every 12ms and repaints only when dirty.
- Closing the window kills spawned terminal child processes.

## 5. Commands

Run the app:

```bash
cargo run -p app
```

Domain tests:

```bash
cargo test -p canvas -p terminal -p runtime
```

Workspace tests:

```bash
cargo test --workspace
```

Code line-count lint:

```bash
bash scripts/check-max-file-lines.sh
```

No `#[allow(...)]` lint:

```bash
bash scripts/check-no-allow.sh
```

Install local Git hooks:

```bash
bash scripts/install-git-hooks.sh
```

App check:

```bash
cargo check -p app
```

Release build:

```bash
cargo build -p app --release
```

Packaging:

```bash
cargo install cargo-packager --locked
cargo build --release -p app
cargo packager --release
```

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

## 6. Verification Matrix

| Change area | Required verification |
|-------------|-----------------------|
| `crates/canvas/**` | `cargo test -p canvas` |
| `crates/terminal/**` | `cargo test -p terminal` |
| `crates/runtime/**` | `cargo test -p runtime` |
| `crates/ui/**` | `cargo test -p ui` |
| `crates/app/**` | `cargo check -p app` |
| Cross-crate behavior | `cargo test --workspace` |
| GPUI boot, release, packaging | `cargo build -p app --release` |
| Domain dependency discipline | `cargo tree -p canvas -p terminal -p runtime | grep -i gpui` should be empty |
| Code line-count lint | `bash scripts/check-max-file-lines.sh` |
| No `#[allow(...)]` lint | `bash scripts/check-no-allow.sh` |

## 7. Test Requirements

All unit tests should follow the project style already present in the crate tests.

Each test needs a doc comment that states:

- Scenario: one sentence describing the behavior under test.
- Data construction: include important concrete values and formulas.
- Execution process: step-by-step behavior and state changes.
- Expected result: positive and negative acceptance criteria.

Assertions:

- Every `assert!`, `assert_eq!`, and `assert_ne!` must include a failure message.
- Prefer specific assertions for named states over generic loops.
- Cover both what should exist and what should not exist when that distinction matters.
- Boundary numbers such as token budgets, timeouts, viewport transforms, rows/cols, and byte values need comments explaining the calculation.

Example pattern:

```rust
/// Ctrl+C 编码为 ETX(0x03)。
///
/// 数值推导：control code = ascii(letter) & 0x1f
///   'c' = 0x63；0x63 & 0x1f = 0x03
///
/// 预期结果：
///   - 正断言：encode(Ctrl+'c') == [0x03]
///   - 负断言：!= [0x63]（不是裸字符 'c'）
#[test]
fn test_ctrl_c_encodes_to_etx() {
    let out = encode(&KeyInput { key: Key::Char('c'), ctrl: true, alt: false });
    assert_eq!(out, vec![0x03], "Ctrl+C 应编码为 0x03（'c'&0x1f）");
    assert_ne!(out, vec![0x63], "不应是裸 'c'(0x63)，说明 ctrl 修饰未生效");
}
```

## 8. UI And Interaction Notes

- GPUI code belongs in `app` and `ui`.
- Before changing GPUI API usage, elements, styling, actions, async tasks, entities, tests, troubleshooting, or code quality patterns, consult the installed skills from `cnwzhu/gpui-skills`: `gpui-fundamentals`, `gpui-init`, `gpui-elements`, `gpui-styling`, `gpui-actions`, `gpui-async`, `gpui-entities`, `gpui-testing`, `gpui-troubleshooting`, `gpui-patterns`, and `gpui-code-quality`.
- Treat the skills as the first reference for GPUI idioms, but verify against this workspace's pinned `gpui =0.2.2` API and nearby code before committing to an implementation.
- Keep domain behavior in pure structs/functions so tests can run without opening a window.
- Canvas coordinates and screen coordinates are separate. Convert through `Viewport`.
- Node movement takes canvas-space deltas; app-layer pointer movement must divide by zoom.
- Wheel zoom should preserve the canvas point under the cursor.
- Terminal card rendering should consume `NodeView::Term` / `GridSnapshot`, not alacritty internals.
- If adding new controls, keep the interaction model direct, reversible, and visible.

Current theme colors live in `crates/ui/src/theme.rs`.

## 9. Roadmap Pointers

Current README phase placeholders:

- Phase 2: undo/redo and spatial index via `canvas/commands.rs` and `history.rs`.
- Phase 3: multiple terminals, resize, copy/paste, mouse reporting.
- Phase 4: snapshot cache and scrollback virtualization via `runtime/snapshot.rs`.
- Phase 5: persistence and recent projects, likely a new `workspace` crate.
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

When adding a new durable rule, keep overlapping facts synchronized between `AGENTS.md` and `CLAUDE.md`.

## 11. Packaging Notes

`Packager.toml` configures an unsigned macOS app and DMG:

- product name: `Multisoul`
- bundle id: `com.multisoul.desktop`
- formats: `app`, `dmg`
- binary: `target/release/multisoul`
- minimum macOS: `12.0`

Codesigning and notarization are explicitly Phase 6.
