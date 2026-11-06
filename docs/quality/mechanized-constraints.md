# Mechanized Constraints

This document lists repository rules that are enforced by scripts rather than
left as review advice.

## R1 · Rust Source Files Stay At Or Below 500 Lines

| | |
|---|---|
| Script | [`scripts/check-max-file-lines.sh`](../../scripts/check-max-file-lines.sh) |
| Scope | `crates/*/src/**/*.rs` and optional root `src/**/*.rs` |
| Limit | 500 physical lines per source file |
| Excludes | `fixtures/` and `__fixtures__/` paths |
| Failure signal | `ERROR: Rust source file(s) exceed 500 lines.` |
| Fix | Split by responsibility, extract shared helpers, and remove duplication rather than raising the cap. |

Run it manually:

```bash
bash scripts/check-max-file-lines.sh
```

For staged-only feedback:

```bash
bash scripts/check-max-file-lines.sh --staged
```

## R2 · Rust Source Must Not Use `#[allow(...)]`

| | |
|---|---|
| Script | [`scripts/check-no-allow.sh`](../../scripts/check-no-allow.sh) |
| Scope | `crates/*/src/**/*.rs` and optional root `src/**/*.rs` |
| Rule | Any `#[allow(...)]` attribute is rejected. There are no exceptions. |
| Excludes | `fixtures/` and `__fixtures__/` paths |
| Failure signal | `ERROR: #[allow(...)] found in Rust source.` |
| Fix | Fix the root cause: remove unused code, delete unused imports, prefix intentionally unused bindings with `_`, split responsibilities, or group parameters into a context struct. |

Run it manually:

```bash
bash scripts/check-no-allow.sh
```

For staged-only feedback:

```bash
bash scripts/check-no-allow.sh --staged
```

## R3 · Agent Guides Stay At Or Below 300 Lines

| | |
|---|---|
| Script | [`scripts/check-agent-guide-lines.sh`](../../scripts/check-agent-guide-lines.sh) |
| Scope | `AGENTS.md` and `CLAUDE.md` at repository root |
| Limit | 300 physical lines per guide file |
| Navigation hub | `AGENTS.md` must keep `## 4. Where to find` |
| Failure signal | `ERROR: Agent guide file(s) exceed 300 lines.` |
| Fix | Archive expanded prose under `docs/` (for example `docs/references/` or `docs/quality/`) and add a pointer row to `AGENTS.md` §4. Do not raise the cap casually. |

Run it manually:

```bash
bash scripts/check-agent-guide-lines.sh
```

For staged-only feedback:

```bash
bash scripts/check-agent-guide-lines.sh --staged
```

Focused script tests:

```bash
bash scripts/test-check-agent-guide-lines.sh
```

## R4 · Interaction And Shutdown Paths Stay Cheap

| | |
|---|---|
| Script | [`scripts/check-performance-guardrails.sh`](../../scripts/check-performance-guardrails.sh) |
| Scope | Canvas mouse/render files, session snapshot save path, and persistence flush ordering |
| Rule | Drag/pan/render/no-op shutdown paths must not perform SQLite work, session directory scans, or unthrottled repaint loops. |
| Failure signal | `ERROR: performance guardrail violations found.` |
| Fix | Cache data from hooks/events, write persistence only after explicit dirty state, coalesce pointer repaint notifications, and cull invisible nodes before cloning or rendering card bodies. |

Run it manually:

```bash
bash scripts/check-performance-guardrails.sh
```

Bad example:

```rust
fn on_mouse_move(app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>) {
    app.persistence.flush_now(&app.runtime, &app.master_layout)?;
    cx.notify();
}

fn collect_node_sessions(workspace_root: &Path) -> Vec<NodeSession> {
    let session_id = find_codex_session_id(workspace_root);
    // ...
}
```

Good example:

```rust
fn on_mouse_move(app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>) {
    if should_notify_motion_frame(app.interaction.last_motion_notify, Instant::now()) {
        cx.notify();
    }
}

fn collect_node_sessions(agent_session_ids: &HashMap<NodeId, String>) -> Vec<NodeSession> {
    let session_id = agent_session_ids.get(&node_id).cloned();
    // ...
}
```

## R5 · Live Video Wallpaper Stays Low-Copy

| | |
|---|---|
| Script | [`scripts/check-video-wallpaper-performance.py`](../../scripts/check-video-wallpaper-performance.py) |
| Scope | `video_wallpaper.rs`, `video_decoder.rs`, and `chrome.rs` |
| Rule | Live video wallpaper must decode IOSurface-backed 420f CVPixelBuffers and render through `gpui::surface()` from a dedicated decoder thread. |
| Failure signal | `ERROR: video wallpaper performance guardrail violations found.` |
| Fix | Keep CPU RGBA / `RenderImage` paths limited to thumbnails, request Metal + IOSurface output settings, disable sample copies, and do not run the AVAssetReader loop in GPUI's shared background executor. |

Run it manually:

```bash
python3 scripts/check-video-wallpaper-performance.py
```

For staged-compatible feedback:

```bash
python3 scripts/check-video-wallpaper-performance.py --staged
```

## R6 · Domain Catalogs And Public Contracts Stay Aligned

| | |
|---|---|
| Script | [`scripts/check-domain-contracts.py`](../../scripts/check-domain-contracts.py) |
| Scope | ASR catalog/release gate and Agent profile enum/schema/Storage codec |
| Rule | Every built-in ASR model ID must have a release-gate entry; every `AgentProfile` variant must be represented by the Canvas enum, `create_agent` schema/parser, and Storage encoder/decoder. |
| Failure signal | `ERROR: domain contract violations found.` |
| Fix | Register new ASR models with an explicit recommendation state, or update all Agent profile contract surfaces in the same change. |

Run it manually or in CI:

```bash
python3 scripts/check-domain-contracts.py
```

## R7 · Child Processes And Display Resources Have Owners

`python3 scripts/check-resource-lifecycle.py` rejects `Command::new("open")`
followed by `.spawn()` within its function, including builders stored in local
variables, including type annotations and nested argument blocks. Local bindings
are scoped so unrelated shadowing does not count as an `open` command.
Use `.status()`/`.output()` on a background thread and handle both
I/O errors and unsuccessful exit status. Thread/task spawns and source examples
inside comments/strings do not count as processes. `--staged` reads the Git index
so an unstaged fix cannot hide a staged regression.

`cargo lint-resources` runs type-aware Clippy `zombie_processes` and `mem_forget`
as errors over workspace production libraries/binaries. Existing test-only
fixtures that deliberately retain resources are outside this production pass;
the normal all-target Clippy step still checks tests. The source guard closes
Clippy's gap around discarded `open` results and rejects `forget`/`leak` calls
in the vendored DisplayLink implementation.

`python3 scripts/test-check-resource-lifecycle.py` tests the lint itself.
`python3 scripts/test-display-link-lifecycle.py` compiles the actual production
DisplayLink module into an isolated, main-thread macOS probe. It checks two
windows' independent callbacks, direct teardown while running, and real `heap` object counts before
and after 64 create/close cycles plus a two-window recheck. CVDisplayLink,
dispatch source and Mach port counts must remain unchanged. It requires an
active macOS display and permission to inspect the probe; missing native
inspection is a failure, not a skipped/pass result. CI runs both layers.

CoreVideo cannot safely join its IO thread on teardown. A process-wide registry
therefore owns one native link per **encountered display ID**, while each window
unsubscribes and releases its dispatch source. This is bounded by display IDs,
not window/paint counts. It is not zero retained native objects. See
[`NORMA_PATCH.md`](../../vendor/gpui-0.2.2/NORMA_PATCH.md).

These checks prevent the known regressions; they are not a proof of absence of
all memory leaks, ownership cycles, unbounded caches or growth in third-party
libraries. Such regressions still require targeted heap/footprint profiling.

## Local Pre-Commit Hook

Install the tracked native Git hook:

```bash
bash scripts/install-git-hooks.sh
```

The hook runs staged file-size, `#[allow(...)]`, agent-guide line, performance,
video and resource-lifecycle checks first. When staged Rust (including vendored
GPUI), Cargo, or toolchain files change, it also runs:

```bash
cargo clippy --workspace --lib --bins --quiet -- -D clippy::zombie_processes -D clippy::mem_forget
cargo fmt --check
```
