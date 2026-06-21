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
