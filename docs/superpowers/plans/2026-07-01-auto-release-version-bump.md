# Auto Release Version Bump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `cargo packager --release -c Packager.toml` advance the persisted release patch version before building release binaries.

**Architecture:** `Packager.toml` will call a repository shell script from `before-packaging-command`. The script will increment the patch version in `Packager.toml` and `Cargo.toml` only when both files currently agree, then the hook will continue with the release build. `cargo-packager 0.11.8` parses config before running the hook, so the persisted version is prepared for the next invocation while the release build itself sees the bumped Cargo workspace version.

**Tech Stack:** POSIX shell, `awk`, `grep`, `cargo-packager 0.11.8`, Rust workspace metadata.

## Global Constraints

- Do not modify unrelated existing dirty files.
- Keep `Packager.toml` and `[workspace.package] version` synchronized.
- Reject non-`MAJOR.MINOR.PATCH` versions instead of guessing.
- Follow repository shell-test comment style with explicit positive and negative assertions.
- `cargo-packager` does not reload `Packager.toml` after `before-packaging-command`.

---

### Task 1: Version Bump Contract

**Files:**
- Create: `scripts/test-bump-release-version.sh`
- Modify: `scripts/test-packager-config.sh`

**Interfaces:**
- Consumes: `scripts/bump-release-version.sh --config <path> --cargo <path>`
- Produces: focused shell tests that fail before implementation and pass after it.

- [ ] **Step 1: Write the failing bump test**

```bash
#!/usr/bin/env bash
# Focused test for autonomous release version bumping.
#
# Scenario:
#   The release bump script advances a plain semantic patch version and keeps
#   Packager.toml synchronized with the workspace package version.
#
# Data construction:
#   Packager.toml version = 1.2.9
#   Cargo.toml workspace version = 1.2.9
#   Expected next patch version = 1.2.(9 + 1) = 1.2.10
#
# Execution:
#   1. Create temporary Packager.toml and Cargo.toml files.
#   2. Run scripts/bump-release-version.sh against those files.
#   3. Verify both files contain 1.2.10 and no longer contain 1.2.9.
#   4. Run the script against an invalid 1.2 version and verify it fails.
#
# Expected result:
#   - Positive: Packager.toml is updated to 1.2.10.
#   - Positive: Cargo.toml workspace version is updated to 1.2.10.
#   - Negative: stale 1.2.9 versions are absent after a successful bump.
#   - Negative: invalid non-MAJOR.MINOR.PATCH versions are rejected.
```

- [ ] **Step 2: Run test to verify it fails**

Run: `bash scripts/test-bump-release-version.sh`

Expected: FAIL because `scripts/bump-release-version.sh` does not exist yet.

- [ ] **Step 3: Extend config test**

Add assertions to `scripts/test-packager-config.sh` requiring:

```bash
scripts/bump-release-version.sh
cargo build --release -p app
```

in `before-packaging-command`, with bump appearing before build.

- [ ] **Step 4: Run config test to verify it fails**

Run: `bash scripts/test-packager-config.sh`

Expected: FAIL because `Packager.toml` still only runs `cargo build --release -p app`.

### Task 2: Version Bump Implementation

**Files:**
- Create: `scripts/bump-release-version.sh`
- Modify: `Packager.toml`

**Interfaces:**
- Consumes: top-level `version = "X.Y.Z"` from `Packager.toml` and `[workspace.package] version = "X.Y.Z"` from `Cargo.toml`.
- Produces: updated files with `Z + 1`; exits non-zero on invalid or mismatched versions.

- [ ] **Step 1: Implement the minimal bump script**

The script should:

```bash
scripts/bump-release-version.sh --config Packager.toml --cargo Cargo.toml
```

and default to those root paths when no arguments are provided.

- [ ] **Step 2: Wire the packager hook**

Change `Packager.toml`:

```toml
before-packaging-command = "scripts/bump-release-version.sh && cargo build --release -p app"
```

- [ ] **Step 3: Run focused tests**

Run:

```bash
bash scripts/test-bump-release-version.sh
bash scripts/test-packager-config.sh
```

Expected: both PASS.

### Task 3: Docs And Verification

**Files:**
- Modify: `docs/references/packaging.md`

**Interfaces:**
- Consumes: behavior from Task 2.
- Produces: release workflow docs explaining the persisted-version caveat.

- [ ] **Step 1: Document release version behavior**

Add a short note that the hook increments `Packager.toml` and `Cargo.toml` before the release build, and that `cargo-packager` has already parsed config before the hook.

- [ ] **Step 2: Run final verification**

Run:

```bash
bash scripts/test-bump-release-version.sh
bash scripts/test-packager-config.sh
bash scripts/check-agent-guide-lines.sh
```

Expected: all PASS.
