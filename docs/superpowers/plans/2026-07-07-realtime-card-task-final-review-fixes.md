# Realtime Card Task Final Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the three final-review Important findings for realtime card tasks without touching the main repo checkout.

**Architecture:** Keep card-task execution serial and deterministic in the domain queue, but move OpenAI planning onto a background path owned by the app shell. Split "cancel requested" from terminal "cancelled" so cancel tool calls get their own callback while the original enqueue tool call stays attached until the task actually stops at a step boundary.

**Tech Stack:** Rust 2021, GPUI 0.2.2, existing `card_task` / `realtime` / `app` crates, background executor tasks, no new async runtime.

## Global Constraints

- Write tests first and verify they fail before production edits.
- Rust test comments must use the required Chinese structure and every assertion must carry a message.
- No `#[allow(...)]`.
- Keep each Rust source file under 500 lines; split tests or helpers when needed.
- Do not block GPUI foreground with network or long-running planning work.
- Work only inside `/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-04-07`.

---

### Task 1: Add RED coverage for the review findings

**Files:**
- Create: `crates/card_task/tests/unit/queue_cancel_flow_tests.rs`
- Create: `crates/app/tests/unit/app_realtime_review_fix_tests.rs`
- Modify: `crates/card_task/src/lib.rs`
- Modify: `crates/app/src/app/realtime.rs`

**Interfaces:**
- Consumes: existing `CardTaskQueue::cancel_latest`, realtime tool-call routing, `RealtimeControlState::validate_start_config`
- Produces: failing tests for cancel request/terminal split, cancel tool-call callback wiring, stale session rejection, and live start-session tool registration

- [ ] **Step 1: Write failing queue/domain tests**
- [ ] **Step 2: Run `cargo test -p card_task queue_cancel_flow_tests -- --nocapture` and capture the expected failures**
- [ ] **Step 3: Write failing app realtime tests**
- [ ] **Step 4: Run targeted `cargo test -p app app_realtime_review_fix_tests -- --nocapture` and capture the expected failures**

### Task 2: Implement cancel split and callback ownership

**Files:**
- Modify: `crates/card_task/src/model.rs`
- Modify: `crates/card_task/src/queue.rs`
- Modify: `crates/app/src/app/realtime/card_tasks.rs`
- Modify: `crates/app/src/app/realtime/shell.rs`

**Interfaces:**
- Consumes: task ids, enqueue tool_call ids, cancel tool call session/tool_call ids
- Produces: `CancelRequested` event (or equivalent split state), terminal callback preservation for enqueue tool calls, explicit cancel tool-call success/failure replies

- [ ] **Step 1: Change queue cancellation semantics so active cancellation emits a non-terminal request event**
- [ ] **Step 2: Preserve enqueue tool_call ownership until terminal cancellation at the next step boundary**
- [ ] **Step 3: Wire cancel tool calls to immediate tool results using their own session/tool_call ids**
- [ ] **Step 4: Reject cancel when no task exists and reject stale-session tool calls**

### Task 3: Implement non-blocking live planner path

**Files:**
- Create: `crates/app/src/app/realtime/card_task_planning.rs`
- Modify: `crates/app/src/app/realtime/card_tasks.rs`
- Modify: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/src/app/realtime/shell.rs`

**Interfaces:**
- Consumes: `VoicePlanningRequest`, Settings OpenAI API key, Settings card planner model
- Produces: background-planned compiled card-task results plus realtime session tool registration

- [ ] **Step 1: Add a background OpenAI planner job helper that returns compiled card-task plans or planner errors**
- [ ] **Step 2: Spawn that job from realtime card-task enqueue flow and store ready results back into app state**
- [ ] **Step 3: Make `run_next` consume only precomputed production planning results on the UI thread**
- [ ] **Step 4: Register `enqueue_card_task` and `cancel_card_task` in realtime start-session config**

### Task 4: Verify and report

**Files:**
- Create: `.superpowers/sdd/final-review-fix-report.md`

**Interfaces:**
- Consumes: test outputs, fmt outputs, modified file list
- Produces: final fix report with red/green evidence and any remaining concerns

- [ ] **Step 1: Run `cargo test -p card_task`**
- [ ] **Step 2: Run targeted app realtime tests that cover the new behavior**
- [ ] **Step 3: Run `cargo fmt --check` (and `cargo fmt` first if needed)**
- [ ] **Step 4: Write `.superpowers/sdd/final-review-fix-report.md` with RED/GREEN evidence and open items**
