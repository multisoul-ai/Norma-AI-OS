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

## Local Pre-Commit Hook

Install the tracked native Git hook:

```bash
bash scripts/install-git-hooks.sh
```

The hook runs staged file-size, `#[allow(...)]`, and agent-guide line checks first. When staged
Rust, Cargo, or toolchain files changed, it also runs:

```bash
cargo check --workspace --quiet
cargo fmt --check
```
