# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Layout

This repository uses a **single-context** layout:

- `CONTEXT.md` at the repository root contains the shared domain vocabulary.
- `docs/adr/` contains architectural decision records for the system.

Norma OS contains multiple crates, but they form one product domain rather than independent bounded contexts.

## Before exploring, read these

- **`CONTEXT.md`** at the repository root.
- **`docs/adr/`** — read ADRs that affect the area about to be changed.

If either path does not exist, proceed silently. Do not require it to be created upfront. The `/domain-modeling` skill creates these files lazily when terminology or architectural decisions are resolved.

## File structure

```text
/
├── CONTEXT.md
├── docs/
│   └── adr/
└── crates/
```

## Use the glossary's vocabulary

When output names a domain concept—in an issue title, refactor proposal, hypothesis, or test name—use the term defined in `CONTEXT.md`. Do not drift to synonyms the glossary explicitly avoids.

If the required concept is absent, reconsider whether the term belongs to the project or record the gap for `/domain-modeling`.

## Flag ADR conflicts

If proposed work contradicts an existing ADR, surface the conflict explicitly rather than silently overriding the decision.
