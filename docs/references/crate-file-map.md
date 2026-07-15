# Crate File Map

Crate 级文件职责速查。系统拓扑与数据流见 [`ARCHITECTURE.md`](../../ARCHITECTURE.md)。

## `canvas`

Purpose: model the infinite canvas without UI dependencies.

Important files:

- `geometry.rs` - `Vec2`, `CanvasRect`, `Viewport`, and `Transform2D`; convention is `screen = canvas * zoom + pan`.
- `document.rs` - `Document`, node storage, viewport, monotonic node ids, z-index allocation, node movement.
- `node.rs` - `CanvasNode`, `NodeKind`/`CanvasNodeKind`, `NodeType`, `RectNode`, `TermSpec`, `AgentSpec`.
- `selection.rs` - stable ordered selection set based on `BTreeSet`.
- `hit_test.rs` - canvas-space hit testing that returns the highest z-index node.
- `commands.rs` and `history.rs` - Phase 2 placeholders for undo/redo.

## `terminal`

Purpose: turn a real PTY into a neutral grid snapshot that UI can render.

Important files:

- `pty.rs` - `portable-pty` wrapper, read thread, output channel, resize/write/kill.
- `grid.rs` - `alacritty_terminal` wrapper producing `GridSnapshot` with neutral `Cell` values.
- `grid/links.rs` - OSC-8 and bare `http(s)` URL discovery plus lexically safe relative Preview paths.
- `input.rs` - `KeyInput` to PTY byte encoding, including Ctrl, Alt, Enter, arrows.
- `session.rs` - `TerminalSession`, drain loop, status, resize, snapshots, cleanup.
- `message.rs` - PTY channel events.

## `runtime`

Purpose: assemble canvas documents and LiveNode runtimes while keeping policy testable.

Important files:

- `live.rs` - `LiveNode`, `Caps`, `NodeInput`, `NodeResize`, and `NodeView`.
- `nodes/term.rs` - `TermLive`, the terminal-backed live node wrapper.
- `nodes/agent.rs` - Agent command factory/wrapper for terminal-backed live nodes.
- `focus.rs` - `CanvasSelect` and `NodeFocused(node_id)`.
- `route.rs` - pure input routing from mode/event to route action.
- `runtime.rs` - document, selection, focus, `NodeId -> LiveNode` registry, spawn/drain/kill.
- `snapshot.rs` - Phase 4 placeholder for snapshot caching and scrollback virtualization.

## `storage`

Purpose: persist app/workspace state through SQLite without depending on GPUI or app shell code.

Important files:

- `app_store.rs` - user-level `app.sqlite`.
- `project_store.rs` - project-level `.multisoul/project.sqlite`.
- `migrations.rs` - idempotent app/project schema setup.
- `recent_store.rs` - recent workspace records.
- `settings_store.rs` - string settings.
- `canvas_store.rs` - `DocumentSnapshot` round-trip.
- `session_store.rs` - coalesced session checkpoints by `node_id`.
- `agent_definition_store.rs` - persistent project Agent identities, names, runtime profiles, and enabled state.
- `agent_dispatch_store.rs` - durable Agent task journal and queued/submitted/terminal state transitions.

Boundary rules:

- `storage` must not depend on `app`, `runtime`, or `ui`.
- `canvas` and `terminal` must not depend on `storage`.
- Do not write SQLite from `Runtime::drain_all`, PTY output chunks, `canvas_view::on_mouse_move`, `runtime::route`, or `send_node_input`.

## `agent`

Purpose: own Dialogue/CardTask execution and persistent-Agent coordination rules without GPUI or Storage.

Important files:

- `coordination.rs` - per-definition FIFO, managed/untracked dispatch policy, atomic preflight, and structured receipts.
- `identity.rs` - validated Agent call names, preset speech aliases, and deterministic built-in allocation.
- `system/` - Dialogue/CardTask orchestration and dynamic Agent roster injection.
- `tools.rs` - high-level Agent/Card tool schemas and command parsing.

## `asr`

Purpose: own recognition-model identity and installation integrity without GPUI, Sherpa, network access, or app settings.

Important files:

- `model_id.rs` - validated immutable/versioned `AsrModelId`.
- `catalog.rs` - curated Paraformer, Transducer Zipformer, and Zipformer2 CTC descriptors, capabilities, pinned URLs, sizes, hashes, and required files.
- `installation.rs` - manifest validation, safe required-file extraction, atomic directory install, and delete protection.
- `support.rs` - pinned shared Silero VAD artifact validation and atomic installation.
- `model_root.rs` - stable Norma OS Application Support model root.

Boundary rules:

- `asr` must not depend on GPUI, Sherpa, app, runtime, storage, or UI.
- Network download and progress live in `app`; Sherpa recognizer configuration lives in `voice_sherpa`.

## `ui` and `app`

Purpose: GPUI shell and rendering.

Important files:

- `crates/ui/src/theme.rs` - dark theme colors as `0xRRGGBB`.
- `crates/app/src/main.rs` - tracing, GPUI application boot, menus, global key bindings.
- `crates/app/src/app/mod.rs` - `MultisoulApp`, background drain loop, keyboard routing, lifecycle cleanup.
- `crates/app/src/canvas/view.rs` - pan, node drag, click/double-click, viewport culling, node chrome.
- `crates/app/src/motion/mod.rs` - card motion state.
- `crates/app/src/cards/mod.rs` - `Card` trait and static `NodeType -> Card` registry.
- `crates/app/src/cards/chrome.rs` - shared card chrome helpers.
- `crates/app/src/cards/term/` - terminal card renderer, terminal view, and grid painter.
- `crates/app/src/cards/agent/` - agent card renderer.

## Source Organization

**Crate 分能力，文件夹分职责，逻辑沉底、适配上浮。**

1. Pure logic → domain crate (`canvas`, `terminal`, `runtime`, `storage`, `voice`).
2. GPUI binding / side effects → `app` or `ui`.
3. Folder names = responsibility (`route`, `settings`), not tech layer (`handlers`, `views`).

**Key examples — same concept, different tier:**

| What | Domain (model) | Adapter (UI) |
|------|----------------|--------------|
| Canvas | `canvas/document.rs`, `hit_test.rs` | `app/canvas/view.rs` (pan, zoom, drag) |
| Terminal | `terminal/pty.rs`, `grid.rs` | `app/cards/term/paint.rs` |
| Input | `runtime/route.rs` | `app/app/keyboard.rs` |
| Settings | `storage/settings_store.rs` | `app/settings/panel.rs` |

**Directory sketch:**

```text
crates/
├── canvas/src/       geometry, document, hit_test     # 零 GPUI
├── terminal/src/     pty, grid, session
├── runtime/src/      route, focus, nodes/{term,agent}
├── storage/src/      *_store.rs
├── asr/src/          catalog, installation, support        # 零 GPUI/Sherpa/network
└── app/src/          GPUI 壳，按体验域分
    ├── main.rs       boot only
    ├── app/          MultisoulApp, keyboard, lifecycle
    ├── canvas/       pointer interaction（≠ canvas crate）
    ├── cards/term/   terminal painting
    └── settings/
```

Do not put domain logic in `app/`, or GPUI code in domain crates. New code goes directly into the target folder; bulk renames only when splitting files.
