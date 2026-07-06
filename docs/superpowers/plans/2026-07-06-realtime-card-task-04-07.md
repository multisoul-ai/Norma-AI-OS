# Realtime Card Task 04-07 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete MVP slices 04-07 so Always Listening can enqueue asynchronous card tasks, execute card CRUD/send-text commands, and inject task results back into the active realtime conversation.

**Architecture:** Add a pure Rust `card_task` crate for queue state, task lifecycle, planner/runner traits, previous-step target resolution, cancellation, and result summaries. Keep live canvas mutation in `crates/app` through existing Norma card-command execution, keep Realtime wire protocol in `crates/realtime`, and keep OpenAI WebSocket adaptation in `sidecars/openai-realtime`. Realtime and sidecar only transport tool calls/results; only Rust planner + executor may mutate canvas cards.

**Tech Stack:** Rust 2021, GPUI `=0.2.2`, `serde`, `serde_json`, `thiserror`, `crossbeam-channel`, existing `voice::compile_voice_plan`, existing `norma::protocol::CardCommand`, Node `v22+`, TypeScript sidecar, OpenAI Realtime tool calls and `conversation.item.create` + `function_call_output` + `response.create` for tool results.

## Global Constraints

- Worktree path: `/Users/alan/Documents/mybook/multisoul-desktop/.worktrees/realtime-card-task-mvp-04-07`.
- Specs covered by this plan: `docs/design-docs/realtime-card-task-mvp/04-card-task-queue.md`, `05-card-send-text.md`, `06-task-result-callback.md`, and `07-end-to-end-acceptance.md`.
- `runtime/canvas/terminal/storage` must not depend on OpenAI, sidecar, HTTP, WebSocket, `card_task`, or new GPUI logic.
- `voice_sherpa` must not know OpenAI, sidecar, card task, or realtime tool calls.
- `crates/card_task` must not depend on GPUI, `app`, `runtime`, `canvas`, `terminal`, or `storage`.
- OpenAI API Key must not be logged, included in command argv, or included in tool result/error summaries.
- One app process supports at most one active realtime session.
- MVP default realtime model remains `gpt-realtime-2`; card planner model remains configurable from Settings.
- Realtime model and Node sidecar must not directly modify canvas state.
- Card Planner output must be parsed as `voice::VoicePlan` and accepted by `voice::compile_voice_plan` before execution.
- Executor may execute only whitelisted `CardCommand::{Create,List,Focus,Delete,SendText}`.
- High-risk confirmation is explicitly out of scope for this MVP; if `compile_voice_plan` marks `requires_confirmation`, card task still executes.
- GPUI foreground must not perform blocking network IO, long sleep, child-process joins, or CPU-heavy planning.
- Rust source files must remain <= 500 lines and must not use `#[allow(...)]`.
- New/changed unit tests must have Chinese doc comments with scenario, data construction, execution process, expected result, and every assertion must have an explanatory message.

---

## File Structure

- Create `crates/card_task/Cargo.toml`: pure domain crate manifest.
- Create `crates/card_task/src/lib.rs`: public exports.
- Create `crates/card_task/src/model.rs`: ids, request/task/step/state/event/result models.
- Create `crates/card_task/src/queue.rs`: serial queue, cancellation semantics, planner/runner orchestration.
- Create `crates/card_task/src/planner.rs`: planner trait plus OpenAI-compatible JSON parsing helpers.
- Create `crates/card_task/src/executor.rs`: compiled command execution and previous-step target resolution.
- Create `crates/card_task/tests/unit/{model_tests.rs,queue_tests.rs,executor_tests.rs,planner_tests.rs}`.
- Modify `Cargo.toml`: add `crates/card_task` workspace member/dependency.
- Modify `crates/app/Cargo.toml`: depend on `card_task`.
- Create `crates/app/src/app/realtime/card_tasks.rs`: app adapter from realtime tool calls to card task queue, planner job, and card runner.
- Modify `crates/app/src/app/realtime.rs`: store task queue/runtime state and callback bridge.
- Modify `crates/app/src/app/realtime/shell.rs`: drain tool calls and task callbacks from sidecar loop.
- Modify `crates/app/src/app/mod.rs`: add module wiring only if required.
- Create `crates/app/tests/unit/app_realtime_card_task_tests.rs`.
- Modify `crates/realtime/src/protocol.rs`: tool result shape and any missing tool-call/result helpers.
- Modify `crates/realtime/src/session.rs`: user/assistant speaking state and callback queue.
- Create or modify `crates/realtime/tests/unit/callback_tests.rs`.
- Modify `sidecars/openai-realtime/src/main.ts`: map OpenAI function call events to Norma `tool_call`, accept `tool_result`, inject function output, and request a response.
- Modify `sidecars/openai-realtime/test/ipc.test.mjs`: fake WebSocket tests for tool calls and tool results.
- Create `docs/design-docs/realtime-card-task-mvp/acceptance-report.md`: automated/manual acceptance status and remaining live-key checks.

---

### Task 1: `card_task` Domain Queue And Executor Core

**Files:**
- Create: `crates/card_task/Cargo.toml`
- Create: `crates/card_task/src/lib.rs`
- Create: `crates/card_task/src/model.rs`
- Create: `crates/card_task/src/queue.rs`
- Create: `crates/card_task/src/executor.rs`
- Create: `crates/card_task/tests/unit/model_tests.rs`
- Create: `crates/card_task/tests/unit/queue_tests.rs`
- Create: `crates/card_task/tests/unit/executor_tests.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `norma::protocol::{CardCommand, CardCommandResult, CardCommandError, NodeId}`
- Consumes: `voice::{CompiledVoicePlan, ExecutableCommand}`
- Produces: `CardTaskRequest { intent: String, tool_call_id: Option<String> }`
- Produces: `CardTaskQueue::enqueue(request) -> CardTaskId`
- Produces: `CardTaskQueue::cancel_latest() -> Result<CardTaskEvent, CardTaskCancelError>`
- Produces: `CardTaskQueue::run_next(&mut planner, &mut runner) -> Vec<CardTaskEvent>`
- Produces: `CardTaskPlanner` trait returning `Result<CompiledVoicePlan, CardTaskPlannerError>`
- Produces: `CardCommandRunner` trait returning `Result<CardCommandResult, CardCommandError>`

- [ ] **Step 1: Write failing model and queue tests**

Cover these exact cases:
- `enqueue_card_task(intent)` creates one queued task and emits `Queued`.
- serial queue has at most one active task while planning/executing.
- second task remains queued until first task succeeds/fails/cancels.
- planner provider error emits `Failed`.
- malformed/compile planner error emits `Failed`.
- queued task cancellation removes latest queued task and emits `Cancelled`.
- active task cancellation sets `cancel_requested` and stops before the next step.

Run:

```bash
cargo test -p card_task --test unit -- queue
```

Expected before implementation: fail because crate or public types do not exist.

- [ ] **Step 2: Implement core models**

Define:

```rust
pub struct CardTaskRequest {
    pub intent: String,
    pub tool_call_id: Option<String>,
}

pub struct CardTask {
    pub task_id: CardTaskId,
    pub request: CardTaskRequest,
    pub summary: String,
    pub steps: Vec<CardTaskStep>,
    pub state: CardTaskState,
    pub cancel_requested: bool,
}

pub enum CardTaskStep {
    Command(norma::protocol::CardCommand),
    SendTextToPrevious { previous_step: usize, text: String, submit: bool },
    FocusPrevious { previous_step: usize },
}
```

`CardTaskState` and `CardTaskEvent` must match the 04 spec names and carry enough data for result callback summaries.

- [ ] **Step 3: Implement serial queue**

Implement queue behavior:
- `Queued -> Planning -> Executing { step_index } -> Succeeded`.
- planner error, compile error, runner error produce terminal `Failed`.
- queued cancellation removes newest queued task first.
- active cancellation does not interrupt the current runner call; it stops before the next step and emits `Cancelled`.
- already completed steps are not rolled back.

- [ ] **Step 4: Implement executor previous-step resolution**

Execution context stores `created_by_step: HashMap<usize, NodeId>`.
Map:
- `ExecutableCommand::Card(command)` -> `CardTaskStep::Command(command)`.
- `ExecutableCommand::SendTextToPrevious` -> previous-step send.
- `ExecutableCommand::FocusPrevious` -> previous-step focus.

Runner results:
- `CardCommandResult::Created { node_id }` records `created_by_step[step_index] = node_id`.
- missing previous step returns task failed.
- previous step that did not create a node returns task failed.
- `submit=true` and `submit=false` are preserved into `CardCommand::SendText`.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test -p card_task
cargo tree -p card_task | rg -i "gpui|app|runtime|canvas|terminal|storage"; test $? -eq 1
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Commit:

```bash
git add Cargo.toml crates/card_task
git commit -m "feat: add card task queue core"
```

---

### Task 2: Card Planner Adapter And App Runner Integration

**Files:**
- Create: `crates/card_task/src/planner.rs`
- Create: `crates/card_task/tests/unit/planner_tests.rs`
- Create: `crates/app/src/app/realtime/card_tasks.rs`
- Modify: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/src/app/realtime/shell.rs`
- Modify: `crates/app/Cargo.toml`
- Create: `crates/app/tests/unit/app_realtime_card_task_tests.rs`

**Interfaces:**
- Consumes: Task 1 `CardTaskQueue`, `CardTaskPlanner`, `CardCommandRunner`
- Consumes: existing `voice::VoicePlanningRequest`, `voice::VoicePlan`, `voice::compile_voice_plan`
- Consumes: existing `crate::norma::server::execute_card_command_for_area`
- Produces: app method `MultisoulApp::enqueue_realtime_card_task(intent: String, tool_call_id: String, cx: &mut Context<Self>)`
- Produces: app method `MultisoulApp::cancel_realtime_card_task(cx: &mut Context<Self>)`

- [ ] **Step 1: Write failing app integration tests**

Use fake planner and fake runner. Cover:
- app receives card-task intent and enqueues task without blocking chat shell.
- app builds `VoicePlanningRequest` from current canvas summaries: `transcript = intent`, cards from current runtime, focused id, selected ids, and max steps.
- compiled `Create/List/Focus/Delete` commands execute through fake runner in serial order.
- planner error and malformed planner JSON produce failed task event.

Run:

```bash
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

Expected before implementation: fail because module/tests are missing.

- [ ] **Step 2: Implement planner helpers**

`card_task::planner` must expose:
- JSON parse helper from string to `voice::VoicePlan`.
- compile helper that calls `voice::compile_voice_plan`.
- error variants for provider failure, JSON failure, compile rejection, and transcript-only rejection.

Do not perform live OpenAI HTTP on the GPUI foreground path. If an OpenAI HTTP provider is added in this task, it must be a background-only adapter and tests must use a fake provider/server; no real API key is required.

- [ ] **Step 3: Implement app card runner**

App runner calls existing `execute_card_command_for_area` with:
- `area = self.visible_canvas_rect(window)`
- `zoom = self.runtime.document.viewport.zoom`
- existing `self.runtime`, `self.motion`, `self.master_layout`

Apply `NormaCommandEffects` exactly like existing voice execution:
- `canvas_dirty` -> `self.mark_canvas_dirty()`
- `session_dirty` -> `self.mark_session_dirty()`
- notify handled by caller after event drain.

- [ ] **Step 4: Implement app queue host**

Realtime control state stores queue/task events but does not mutate canvas directly.
`MultisoulApp` owns the adapter that:
- creates planning request from app state,
- runs planning in background if provider is live,
- executes runner steps on foreground only,
- keeps the chat shell active while tasks execute.

MVP can use deterministic fake planner in tests. Production provider may initially return a failed task if OpenAI planner settings are unavailable, but it must not panic or block foreground.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test -p card_task
cargo test -p app app_realtime_card_task_tests -- --nocapture
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
```

Commit:

```bash
git add crates/card_task crates/app/Cargo.toml crates/app/src/app/realtime.rs crates/app/src/app/realtime crates/app/tests/unit/app_realtime_card_task_tests.rs
git commit -m "feat: integrate card task queue with realtime app"
```

---

### Task 3: Realtime Tool Call Protocol And Sidecar Mapping

**Files:**
- Modify: `crates/realtime/src/protocol.rs`
- Modify: `crates/realtime/tests/unit/protocol_tests.rs`
- Modify: `sidecars/openai-realtime/src/main.ts`
- Modify: `sidecars/openai-realtime/test/ipc.test.mjs`
- Modify: `crates/app/src/app/realtime/shell.rs`
- Modify: `crates/app/tests/unit/app_realtime_card_task_tests.rs`

**Interfaces:**
- Consumes: existing `RealtimeEvent::ToolCall(RealtimeToolCall)`
- Consumes: app methods from Task 2
- Produces: sidecar `tool_call` JSONL events for `enqueue_card_task` and `cancel_card_task`
- Produces: failed tool result path for unknown tool names or missing `intent`

- [ ] **Step 1: Write failing sidecar fake WebSocket tests**

Cover:
- OpenAI function-call completion for `enqueue_card_task` emits JSONL:

```json
{"type":"tool_call","session_id":"always-listening-shell","tool_call_id":"call_1","name":"enqueue_card_task","arguments":{"intent":"创建 Codex 卡片"}}
```

- `cancel_card_task` emits tool call with `name="cancel_card_task"`.
- missing/invalid arguments still emit a tool call so Rust can return a failed tool result.
- no OpenAI API key appears in emitted errors.

Run:

```bash
cd sidecars/openai-realtime
npm test
```

Expected before implementation: fail because function-call mapping is missing.

- [ ] **Step 2: Implement sidecar function-call mapping**

Handle current and legacy Realtime event shapes:
- `response.output_item.done` with `item.type == "function_call"`.
- `response.function_call_arguments.done`.
- any existing fake shape already used by tests.

Extract:
- `call_id` or `id` as `tool_call_id`.
- function `name`.
- JSON `arguments`; malformed JSON becomes `{}` and lets Rust fail with a clean tool result.

- [ ] **Step 3: Route tool calls in app**

In app sidecar drain:
- `enqueue_card_task` with non-empty string `intent` calls `enqueue_realtime_card_task`.
- `cancel_card_task` calls `cancel_realtime_card_task`.
- unknown tool name or missing intent creates a failed `RealtimeToolResult` through callback/result path.

Do not let sidecar or realtime crate mutate cards directly.

- [ ] **Step 4: Verify and commit**

Run:

```bash
cd sidecars/openai-realtime && npm run build && npm test
cargo test -p realtime --test unit -- protocol
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

Commit:

```bash
git add crates/realtime sidecars/openai-realtime crates/app/src/app/realtime crates/app/tests/unit/app_realtime_card_task_tests.rs
git commit -m "feat: route realtime card task tool calls"
```

---

### Task 4: SendText Previous-Step MVP And Cancellation Semantics

**Files:**
- Modify: `crates/card_task/src/executor.rs`
- Modify: `crates/card_task/src/queue.rs`
- Modify: `crates/card_task/tests/unit/executor_tests.rs`
- Modify: `crates/card_task/tests/unit/queue_tests.rs`
- Modify: `crates/app/tests/unit/app_realtime_card_task_tests.rs`

**Interfaces:**
- Consumes: Task 1 previous-step support
- Produces: full 05 spec support for create -> send, focus previous, submit preservation, and cancel after create before send

- [ ] **Step 1: Write failing send-text tests**

Cover exact 05 acceptance:
- create -> send previous-step preserves created `node_id`.
- `SendTextToPrevious` references nonexistent step -> task failed.
- `SendTextToPrevious` references non-create step -> task failed.
- `submit=true` passes through to `CardCommand::SendText`.
- `submit=false` passes through without synthetic newline at `card_task` layer.
- send failure stops later steps.
- active task cancelled after create and before send does not delete created card and does not send text.

Run:

```bash
cargo test -p card_task --test unit -- executor
cargo test -p card_task --test unit -- queue
```

- [ ] **Step 2: Tighten executor and cancellation behavior**

Ensure step boundary cancellation check happens:
- before planning starts if active task already cancelled,
- before each step after the previous step event has been emitted,
- never mid-runner call.

Ensure cancellation summary mentions completed work is not rolled back when at least one step succeeded.

- [ ] **Step 3: Verify and commit**

Run:

```bash
cargo test -p card_task
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

Commit:

```bash
git add crates/card_task crates/app/tests/unit/app_realtime_card_task_tests.rs
git commit -m "feat: support card task send text steps"
```

---

### Task 5: Task Result Callback Queue And Tool Result Injection

**Files:**
- Modify: `crates/realtime/src/session.rs`
- Modify: `crates/realtime/src/protocol.rs`
- Create: `crates/realtime/tests/unit/callback_tests.rs`
- Modify: `crates/realtime/tests/unit/protocol_tests.rs`
- Modify: `crates/app/src/app/realtime.rs`
- Modify: `crates/app/src/app/realtime/shell.rs`
- Modify: `crates/app/src/app/realtime/card_tasks.rs`
- Modify: `sidecars/openai-realtime/src/main.ts`
- Modify: `sidecars/openai-realtime/test/ipc.test.mjs`

**Interfaces:**
- Consumes: `CardTaskEvent::{Succeeded, Failed, Cancelled}` plus stored `tool_call_id`
- Produces: `RealtimeToolResult` JSONL:

```json
{"type":"tool_result","session_id":"always-listening-shell","tool_call_id":"call_123","result":{"status":"succeeded","task_id":"task_7","summary":"已创建 Codex 卡片并发送请求。"}}
```

- Produces: callback queue gated by `user_speaking == false && assistant_speaking == false`

- [ ] **Step 1: Write failing realtime callback tests**

Cover:
- idle callback injects immediately.
- user speaking queues callback.
- assistant speaking queues callback.
- transcript final flushes pending callback.
- assistant audio end flushes pending callback.
- success/failed/cancelled statuses encode to tool result JSONL.
- queue limit drops oldest callback and logs/returns a drop event without panic.

Run:

```bash
cargo test -p realtime --test unit -- callback
```

- [ ] **Step 2: Implement callback queue in `realtime`**

Track:
- `user_speaking`
- `assistant_speaking`
- `active_response_id` if already available from sidecar event metadata; otherwise keep field optional for future use.
- pending callbacks in completion order.

Do not restart stopped sessions. If no active session/tool call id exists, return a non-panicking error to app.

- [ ] **Step 3: Implement sidecar `tool_result` command**

On Rust `tool_result` command:
- send `conversation.item.create` with item type `function_call_output`, `call_id = tool_call_id`, and output JSON string.
- then send `response.create` so the model naturally reports the result.
- sanitize API key from all errors.

Use official OpenAI Realtime semantics from current docs: function output is added through `conversation.item.create`, then a response is requested with `response.create`.

- [ ] **Step 4: Wire app task completion to callback queue**

When card task reaches terminal state:
- build `RealtimeToolResult` with status `succeeded`, `failed`, or `cancelled`.
- enqueue/inject based on realtime speaking state.
- if realtime unavailable, log only and do not panic.

- [ ] **Step 5: Verify and commit**

Run:

```bash
cargo test -p realtime
cd sidecars/openai-realtime && npm run build && npm test
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

Commit:

```bash
git add crates/realtime sidecars/openai-realtime crates/app/src/app/realtime crates/app/tests/unit/app_realtime_card_task_tests.rs
git commit -m "feat: inject card task results into realtime"
```

---

### Task 6: End-To-End Acceptance Harness And Documentation

**Files:**
- Create: `docs/design-docs/realtime-card-task-mvp/acceptance-report.md`
- Modify: `docs/design-docs/realtime-card-task-mvp/07-end-to-end-acceptance.md`
- Modify: `crates/app/tests/unit/app_realtime_card_task_tests.rs`
- Modify: `crates/app/tests/unit/app_realtime_chat_tests.rs` only if needed for regression coverage.

**Interfaces:**
- Consumes: Tasks 1-5 implemented behavior
- Produces: automated MVP acceptance evidence for 07 spec
- Produces: clear manual-live-key checklist for OpenAI microphone/speaker validation

- [ ] **Step 1: Write failing app-level acceptance test**

Create one deterministic test covering:
- fake realtime tool call `enqueue_card_task` with intent “创建一个 Codex 卡片，让它分析 README，然后发送”.
- fake planner returns create Codex + send previous-step submit=true.
- fake runner records `Create Agent(Codex)` then `SendText { node_id: created_id, text contains README, submit: true }`.
- task terminal event becomes realtime tool result with `status="succeeded"`.
- while assistant speaking, result queues; after assistant audio end, result injects.

Run:

```bash
cargo test -p app app_realtime_card_task_tests -- --nocapture
```

- [ ] **Step 2: Add automated acceptance report**

Document:
- which 07 checklist items are covered by automated tests,
- which require live OpenAI key + microphone/speaker manual run,
- exact command list with latest local verification outputs written after commands run.

- [ ] **Step 3: Run final required verification**

Run:

```bash
cargo test -p voice
cargo test -p voice_sherpa
cargo test -p card_task
cargo test -p realtime
cargo test -p app
cargo check -p app
cd sidecars/openai-realtime && npm run build && npm test
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
cargo fmt --check
git diff --check
```

If live OpenAI API key is not available, do not block automated completion. Mark live speech/manual validation as not run in `acceptance-report.md`.

- [ ] **Step 4: Commit**

```bash
git add docs/design-docs/realtime-card-task-mvp crates/app/tests/unit/app_realtime_card_task_tests.rs
git commit -m "test: add realtime card task acceptance coverage"
```

---

## Final Review

After Task 6:

1. Run `git merge-base dev HEAD` and create a final review package for the whole branch.
2. Dispatch final code review against this plan and specs 04-07.
3. Fix Critical/Important findings through one fix subagent.
4. Re-run the final verification command list.
5. Update `.superpowers/sdd/progress.md` with all completed task ranges and review verdicts.
