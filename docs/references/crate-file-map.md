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

Purpose: own provider registry, recognition protocol, Local Sherpa, Cloud Azure, model identity, and installation integrity without GPUI, Storage, Keychain, or app settings.

Important files:

- `model_id.rs` - validated immutable/versioned `AsrModelId`.
- `catalog.rs` - curated Paraformer, Transducer Zipformer, and Zipformer2 CTC descriptors, capabilities, pinned URLs, sizes, hashes, and required files.
- `installation.rs` - manifest validation, safe required-file extraction, atomic directory install, and delete protection.
- `support.rs` - pinned shared Silero VAD artifact validation and atomic installation.
- `model_root.rs` - stable Norma OS Application Support model root.
- `providers/` - built-in Local Sherpa and Cloud Azure provider implementations and registry.
- `recognition.rs` - provider-neutral bounded recognition pipeline and terminal event protocol.

Boundary rules:

- `asr` must not depend on GPUI, app, runtime, storage, UI, or Keychain.
- Provider network execution belongs in `asr`; credential storage and runtime composition remain in `app`.
- Provider-neutral microphone capture, PCM normalization, and VAD live in `voice_input`.

## `model_store` and `tts`

Purpose: keep common local-model installation mechanics separate from provider-neutral TTS policy and built-in remote protocols.

- `model_store/src/lib.rs` and `installation.rs` - archive SHA-256, safe extraction, manifest validation, supplemental artifacts, and atomic promotion. It has no GPUI, network, app, runtime, storage, or Sherpa dependency.
- `tts/src/model_id.rs` - stable `TtsModelId` and `TtsVoiceId`.
- `tts/src/catalog.rs` - curated model packages, capability metadata, backend family, and recommended voices.
- `tts/src/installation.rs` - TTS-specific root paths and validation over `model_store` primitives.
- `tts/src/provider.rs` - stable Provider IDs, built-in registry, and curated model descriptors.
- `tts/src/profile.rs`, `voice_catalog.rs` - provider selections, validation-bound readiness, and dynamic Voice catalogs.
- `tts/src/stream.rs` - reply-level start/push/finish/cancel seam and ordered PCM sink.
- `tts/src/providers/minimax/` - MiniMax account Voice query plus one-WebSocket-per-reply protocol and transport.

Boundary rules:

- `tts -> {model_store, audio}`; neither model policy nor provider code depends on GPUI, app, runtime, storage, or UI.
- `tts` does not depend on Sherpa; `voice_sherpa` consumes validated TTS models to create the engine adapter.
- Network dependencies are contained below `tts::providers`; credentials and activation remain App responsibilities.
- Network transfer, progress, cancellation, selection, and deletion orchestration live in `app`.

## `ui` and `app`

Purpose: GPUI shell and rendering.

Important files:

- `crates/ui/src/theme.rs` - dark theme colors as `0xRRGGBB`.
- `crates/app/src/lib.rs` - logging initialization, GPUI application boot, menus, global key bindings.
- `crates/app/src/logging.rs` - stderr/file tracing layers, daily rolling files, retention maintenance.
- `crates/app/src/app/mod.rs` - `MultisoulApp`, background drain loop, keyboard routing, lifecycle cleanup.
- `crates/app/src/canvas/view.rs` - pan, node drag, click/double-click, viewport culling, node chrome.
- `crates/app/src/motion/mod.rs` - card motion state.
- `crates/app/src/cards/mod.rs` - `Card` trait and static `NodeType -> Card` registry.
- `crates/app/src/cards/chrome.rs` - shared card chrome helpers.
- `crates/app/src/cards/term/` - terminal card renderer, terminal view, and grid painter.
- `crates/app/src/cards/agent/` - agent card renderer.
- `crates/app/src/app/browser/` - Browser state, Wry/CEF adapters, and explicit Chrome Cookie import broker/protocol.
- `crates/app/src/browser_auth_host.rs` - Chrome Native Messaging framing, user-level host manifest, and bundled extension discovery.

## Source Organization

**Crate 分能力，文件夹分职责，逻辑沉底、适配上浮。**

1. Pure logic → domain crate (`canvas`, `terminal`, `runtime`, `storage`, `model_store`, `tts`, `voice`).
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
├── asr/src/          providers, recognition, installation  # 零 GPUI/Storage/Keychain
├── voice_input/src/  microphone, PCM, resample, VAD        # provider 中立
├── model_store/src/  installation                             # 零 GPUI/network
├── tts/src/          model_id, catalog, installation          # 零 GPUI/Sherpa
└── app/src/          GPUI 壳，按体验域分
    ├── main.rs       boot only
    ├── app/          MultisoulApp, keyboard, lifecycle
    ├── canvas/       pointer interaction（≠ canvas crate）
    ├── cards/term/   terminal painting
    └── settings/
```

Do not put domain logic in `app/`, or GPUI code in domain crates. New code goes directly into the target folder; bulk renames only when splitting files.
