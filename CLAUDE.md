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
app -> {ui, runtime, canvas, terminal, storage}
runtime -> {canvas, terminal, storage}
storage -> {canvas, workspace, rusqlite}
ui -> gpui
```

`canvas`, `terminal`, and `runtime` must remain free of GPUI dependencies and should stay independently testable.

## 2. Key Constraints

- **Domain crates stay UI-free**: no `gpui` imports in `canvas`, `terminal`, or `runtime`.
- **Input decisions stay pure where possible**: encode routing decisions in `runtime::route` before app-layer side effects.
- **PTY cleanup is required**: `Runtime::kill_all`, `TerminalSession::kill`, and `PtyHandle::Drop` are lifecycle guardrails. Keep new process paths equally explicit.
- **Quit on last window close**: GPUI on macOS does not quit the process automatically when the last window closes. `main.rs` must register `cx.on_window_closed(|cx| { if cx.windows().is_empty() { cx.quit(); } }).detach()` inside the `Application::run` closure, before `open_window`. Omitting this leaves the process running invisibly in the background after the user closes the window.
- **Pinned libraries require verification**: `gpui =0.2.2` and `alacritty_terminal =0.26` are pinned in the workspace. Upgrade only after checking API compatibility.
- **Consult GPUI skills first**: when writing GPUI code in `crates/app` or `crates/ui`, load the relevant local `gpui-*` skill before inventing a pattern, then adapt it to this repository's pinned GPUI version and existing style.
- **New configurable app shortcuts must appear in Settings**: register them in `shortcuts.rs` and support recording/restoring them in Settings → Keyboard Shortcuts.
- **TDD before feature or bugfix work**: add a focused failing test first where the behavior can be tested below the GPUI layer.
- **No `#[allow(...)]`**: do not hide Rust diagnostics with allow attributes. Fix root causes instead.
- **Single Rust source files stay at or below 500 lines**: enforced by [`scripts/check-max-file-lines.sh`](scripts/check-max-file-lines.sh); split responsibilities instead of raising the cap.
- **Agent guides stay at or below 300 lines**: `AGENTS.md` and `CLAUDE.md` are enforced by [`scripts/check-agent-guide-lines.sh`](scripts/check-agent-guide-lines.sh); archive detail to `docs/` and link from [`AGENTS.md`](AGENTS.md) §4.
- **Native pre-commit is available**: run `bash scripts/install-git-hooks.sh` to enable `.githooks/pre-commit`, which runs staged constraints plus lightweight Rust checks.
- **Keep source files small enough to navigate**: split before a file turns into a mixed-responsibility dumping ground.
- **Do not treat `.superpowers/sdd/` as the new documentation home**: it is useful historical construction context. New durable specs/plans should use a future `docs/` tree.

## 3. Architecture Summary

Canonical architecture details live in [`ARCHITECTURE.md`](ARCHITECTURE.md). Per-crate file maps and source organization live in [`docs/references/crate-file-map.md`](docs/references/crate-file-map.md).

At a glance:

- `canvas` / `terminal` / `runtime` / `storage` stay GPUI-free and independently testable.
- `app` binds GPUI, canvas interaction, cards, menus, and keyboard routing.
- Input policy is expressed in `runtime::route` before app-layer side effects.
- SQLite must not run on PTY drain, mouse move, or input routing hot paths.

## 4. Current User-Facing Behavior

- `Cmd+N` creates a terminal node, then reflows all nodes into a row-major tiled grid with at most three columns.
- Drag a node by its title bar.
- Single-click selects a node; clicking blank canvas clears selection.
- Double-click a focusable node, or click a terminal/agent body, to focus it.
- `Esc` exits node focus.
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
cargo test -p canvas -p terminal -p runtime -p storage
```

Workspace tests and app check:

```bash
cargo test --workspace
cargo check -p app
```

Mechanized constraints:

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
bash scripts/check-storage-boundaries.sh
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

## 8. UI And Interaction Notes

- GPUI code belongs in `app` and `ui`.
- Before changing GPUI API usage, elements, styling, actions, async tasks, entities, tests, troubleshooting, or code quality patterns, consult the installed skills from `cnwzhu/gpui-skills`: `gpui-fundamentals`, `gpui-init`, `gpui-elements`, `gpui-styling`, `gpui-actions`, `gpui-async`, `gpui-entities`, `gpui-testing`, `gpui-troubleshooting`, `gpui-patterns`, and `gpui-code-quality`.
- Treat the skills as the first reference for GPUI idioms, but verify against this workspace's pinned `gpui =0.2.2` API and nearby code before committing to an implementation.
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
<!-- msctl-inject-start -->
## msctl Quick Reference

### Agent

```bash
msctl agent list             # List all registered agents
msctl agent get <id>         # Show agent details
msctl agent codex            # Quick register current project with Codex
msctl agent claude-code      # Quick register current project with Claude Code
msctl agent cursor-cli       # Quick register current project with Cursor
msctl agent kodax            # Quick register current project with KodaX
msctl agent register --name <name> --project <path> --runtime claude-code
msctl agent register --name <name> --project <path> --runtime codex
msctl agent register --name <name> --project <path> --runtime cursor-cli
msctl agent register --name <name> --project <path> --runtime kodax
msctl agent invoke <id> --message "<task>"
msctl agent delete <id>
```

### Ask User Question (`msctl ask-question`)

**MANDATORY**

- For structured decisions (approach, continue/stop, risk trade-offs, etc.), **prefer** the `AskUserQuestion` tool when available.
- **If the current runtime does not expose `AskUserQuestion`, you MUST call `msctl ask-question` to push a question card to iOS.** Never list options in free text or ask the user to type an answer.
- After pushing, continue other work. Do not poll; iOS answers are injected into the same conversation automatically.

Run `msctl ask-question -h` for flags, JSON shape, and copy-paste examples.

### Image Output

When generating images (charts, screenshots, diagrams), save them as files and
reference them in your reply using standard Markdown image syntax:

![description](/absolute/path/to/image.png)

Supported formats: png, jpg, jpeg, gif, webp.
The MultiSoul mobile app will automatically render these images inline.

<!-- msctl-inject-end -->
