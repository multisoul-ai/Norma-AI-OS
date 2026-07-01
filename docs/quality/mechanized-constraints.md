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

## Local Pre-Commit Hook

Install the tracked native Git hook:

```bash
bash scripts/install-git-hooks.sh
```

The hook runs staged file-size, `#[allow(...)]`, agent-guide line, and performance guardrail checks first. When staged Rust, Cargo, or toolchain files changed, it also runs:

```bash
cargo check --workspace --quiet
cargo fmt --check
```
