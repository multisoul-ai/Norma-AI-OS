# Local Voice Pipeline 02 — LLM Client And Card Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `LlmClient` (Mock first, then Ollama), connect Thinking→reply→TTS, flatten card tools to `Create`/`Focus`/`SendText` with multi-round `node_id` handoff, template TTS reports, and rewrite acceptance tests away from Realtime.

**Architecture:** New GPUI-free `crates/llm` owns chat/tool-calling traits. App `DialogueSession` orchestration calls `LlmClient` after ASR finals. Tool calls enqueue **already-resolved** `CardCommand` steps on `card_task` (no Planner / no `enqueue_card_task(intent)`). Create tool results return `node_id` into the same LLM turn for Follow-up Focus/SendText. Task completion uses template strings + `dialogue::decide_template_report` before TTS.

**Tech Stack:** Rust 2021, existing `voice_sherpa` TTS/ASR/VAD from plan 01, Ollama HTTP (`/api/chat` with tools), `serde_json`, `card_task`, `norma::protocol::CardCommand`, GPUI app shell.

**Spec:** [`docs/product-specs/2026-07-09-SPEC-local-voice-pipeline.md`](../../product-specs/2026-07-09-SPEC-local-voice-pipeline.md) §6.4 steps 5–8.

**Prerequisite:** Plan 01 exit criteria met (`docs/superpowers/plans/2026-07-09-local-voice-pipeline-01-speech-skeleton.md`).

---

## Global Constraints

- Do not reintroduce OpenAI Realtime, sidecar, or `enqueue_card_task(intent)` + Planner as the Always Listening path.
- Existing one-shot voice planner in `crates/voice` (`OllamaVoicePlanner` / `compile_voice_plan`) may remain for push-to-talk card creation until a later cleanup; **Always Listening must not call it**.
- MVP tools only: `Create`, `Focus`, `SendText`. No `List`/`Delete`/cancel tool in this plan.
- Barge-in stops TTS only; card tasks keep running. Closing Listening cancels unfinished tasks; models stay resident.
- Ollama unavailable ⇒ Always Listening start fails with visible bar error (after Mock path is swapped to Ollama in production settings).
- Tests: Chinese scenario comments + assertion messages; no `#[allow(...)]`; files ≤ 500 lines.
- Prefer TDD. Commit steps are for when the user requests commits.

---

## File Structure

Create:

- `crates/llm/Cargo.toml`
- `crates/llm/src/lib.rs`
- `crates/llm/src/types.rs` — messages, tool defs, tool calls, errors
- `crates/llm/src/client.rs` — `LlmClient` trait
- `crates/llm/src/mock.rs` — scripted Mock client
- `crates/llm/src/ollama.rs` — Ollama chat + tools adapter
- `crates/llm/src/tools_card.rs` — JSON schema / parse helpers for Create/Focus/SendText
- `crates/llm/tests/unit/mock_tests.rs`
- `crates/llm/tests/unit/ollama_tests.rs` (HTTP fakes; ignored live test optional)
- `crates/llm/tests/unit/tools_card_tests.rs`
- `crates/dialogue/src/turn.rs` — pure turn driver: transcript → LLM → tool loop → reply text
- `crates/dialogue/tests/unit/turn_tests.rs`
- `crates/app/src/app/dialogue/chat.rs` — background LLM turn + TTS
- `crates/app/src/app/dialogue/card_tools.rs` — tool → `card_task` enqueue + template reports
- `crates/app/tests/unit/app_dialogue_turn_tests.rs`
- `crates/app/tests/unit/app_dialogue_card_tool_tests.rs`
- `crates/e2e/tests/local_voice_card_tools_pipeline.rs`

Modify:

- `Cargo.toml` — add `llm` member/dep
- `crates/card_task/src/model.rs`, `queue.rs`, `lib.rs` — enqueue resolved commands without requiring Planner
- `crates/card_task/tests/unit/**` — add flat-enqueue tests; keep or gate old planner tests
- `crates/app/src/settings/voice/**` — Ollama endpoint/model; remove obsolete Realtime fields if any remain
- `crates/app/src/app/dialogue/shell.rs` — require LLM readiness when not using Mock
- `docs/design-docs/realtime-card-task-mvp/README.md` — superseded banner (if not done in plan 01)

Do not modify:

- `crates/runtime`, `canvas`, `terminal`, `storage` domain rules
- Phase-2 List/Delete tools

---

### Task 1: `crates/llm` Types, Trait, And Mock Client

**Files:**
- Create: `crates/llm/**` core + mock + tests
- Modify: `Cargo.toml`

**Interfaces:**

```rust
// crates/llm/src/types.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatResponse {
    pub assistant_text: String,
    pub tool_calls: Vec<ToolCall>,
}
```

```rust
// crates/llm/src/client.rs
pub trait LlmClient: Send {
    fn chat(&mut self, request: &ChatRequest) -> Result<ChatResponse, LlmError>;
}
```

Mock DSL (keep small):

```rust
pub enum MockStep {
    Text(String),
    Tools(Vec<ToolCall>),
    TextAndTools { text: String, tools: Vec<ToolCall> },
}

pub struct MockLlmClient {
    steps: std::collections::VecDeque<MockStep>,
}
```

Each `chat` pops one step. If empty → `LlmError::Message("mock exhausted")`.

- [ ] **Step 1: Write failing mock tests**

```rust
/// MOCK-1: 第一次 chat 返回 tool_calls，第二次返回最终文本。
#[test]
fn test_mock_llm_returns_tools_then_text_across_two_chats() {
    let mut client = MockLlmClient::script(vec![
        MockStep::Tools(vec![ToolCall {
            id: "call_1".into(),
            name: "Create".into(),
            arguments_json: r#"{"kind":"terminal"}"#.into(),
        }]),
        MockStep::Text("已经创建终端".into()),
    ]);
    let req = ChatRequest {
        messages: vec![],
        tools: vec![],
    };
    let first = client.chat(&req).expect("first scripted step");
    assert_eq!(first.tool_calls.len(), 1, "first step must expose Create tool call");
    assert!(first.assistant_text.is_empty(), "tool-only step should not carry user-facing text");
    let second = client.chat(&req).expect("second scripted step");
    assert_eq!(second.assistant_text, "已经创建终端", "second step must return final reply text");
    assert!(second.tool_calls.is_empty(), "final text step must not invent extra tools");
}
```

- [ ] **Step 2: Run — expect FAIL**

Run: `cargo test -p llm -- --nocapture`

- [ ] **Step 3: Implement types + Mock**

- [ ] **Step 4: Run — expect PASS**

Run: `cargo test -p llm`

- [ ] **Step 5: Commit** (when requested)

```bash
git add Cargo.toml crates/llm
git commit -m "$(cat <<'EOF'
Add llm crate with ChatRequest types and scripted MockLlmClient.

EOF
)"
```

---

### Task 2: Card Tool Schema Parse Helpers

**Files:**
- Create: `crates/llm/src/tools_card.rs`
- Create: `crates/llm/tests/unit/tools_card_tests.rs`

**Interfaces:**

Map tool names to `norma::protocol::CardCommand` (MVP subset):

- `Create` args → `CardCommand::Create { kind, placement }` (define a **small** JSON shape; e.g. `{ "kind": "terminal" }` with default placement used by app today)
- `Focus` → `{ "node_id": 12 }`
- `SendText` → `{ "node_id": 12, "text": "...", "submit": true }`

```rust
pub fn card_tool_specs() -> Vec<ToolSpec> { /* three tools */ }

pub fn parse_card_tool_call(call: &ToolCall) -> Result<CardCommand, LlmError> { /* ... */ }

pub fn tool_result_json_for_command_result(
    result: &norma::protocol::CardCommandResult,
) -> String {
    // Create must include node_id field for the next LLM round
}
```

- [ ] **Step 1: Failing parse tests**

Cover valid Create/Focus/SendText; reject unknown tool name; reject SendText missing `node_id`; Create result JSON contains `node_id`.

- [ ] **Step 2: Implement helpers** (`llm` may depend on `norma` for `CardCommand`)

- [ ] **Step 3: `cargo test -p llm`**

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Add Create/Focus/SendText tool schema helpers for LlmClient.

EOF
)"
```

---

### Task 3: Pure Multi-Round Turn Driver In `dialogue`

**Files:**
- Create: `crates/dialogue/src/turn.rs`
- Modify: `crates/dialogue/src/lib.rs`, `Cargo.toml` (depend on `llm`)
- Create: `crates/dialogue/tests/unit/turn_tests.rs`

**Interfaces:**

```rust
pub struct TurnOutcome {
    pub reply_text: String,
    pub executed_commands: Vec<norma::protocol::CardCommand>,
    pub command_results: Vec<norma::protocol::CardCommandResult>,
}

pub trait ToolExecutor {
    fn execute(
        &mut self,
        command: norma::protocol::CardCommand,
    ) -> Result<norma::protocol::CardCommandResult, DialogueError>;
}

/// Runs chat → tools → tool results → chat until assistant_text is final or max_rounds.
pub fn run_voice_turn<L: LlmClient, E: ToolExecutor>(
    llm: &mut L,
    executor: &mut E,
    user_transcript: &str,
    system_prompt: &str,
    max_tool_rounds: usize,
) -> Result<TurnOutcome, DialogueError> {
    // 1) push system + user
    // 2) loop:
    //    - llm.chat with card_tool_specs()
    //    - if tool_calls empty: return assistant_text
    //    - else for each tool: parse → executor.execute → append tool role message with result JSON
    // 3) if max rounds exceeded: error
}
```

- [ ] **Step 1: Failing turn test with Mock + fake executor**

Scenario: user「创建终端并发送 ls」  
Mock step1: Create tool  
Fake executor returns `Created { node_id: 7 }`  
Mock step2: SendText `{node_id:7,text:"ls",submit:true}`  
Fake executor returns `Sent`  
Mock step3: text「好的，已经发到终端」  

Assert executed command order and final reply.

- [ ] **Step 2: Implement `run_voice_turn`**

- [ ] **Step 3: `cargo test -p dialogue`**

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Add dialogue multi-round voice turn driver over LlmClient tools.

EOF
)"
```

---

### Task 4: Flatten `card_task` Enqueue For Resolved Commands

**Files:**
- Modify: `crates/card_task/src/model.rs`
- Modify: `crates/card_task/src/queue.rs`
- Modify: `crates/card_task/src/lib.rs`
- Create/Modify tests: `crates/card_task/tests/unit/queue_flat_tests.rs`

**Design:**

Add a path that **skips Planner**:

```rust
pub struct FlatCardTaskRequest {
    pub summary: String,
    pub steps: Vec<CardCommand>, // already concrete; no SendTextToPrevious required for MVP
    pub tool_call_id: Option<String>,
}

impl CardTaskQueue {
    pub fn enqueue_flat(&mut self, request: FlatCardTaskRequest) -> CardTaskId {
        // Queued → directly Planned/Executing with CardTaskStep::Command entries
    }
}
```

Keep old `enqueue` + `CardTaskPlanner` temporarily for one-shot / legacy tests, or convert legacy tests to flat if cheap. Prefer **not** deleting planner module in this task if one-shot still needs it; Always Listening must use `enqueue_flat` only.

- [ ] **Step 1: Failing flat queue test**

Enqueue `[Create, SendText{node_id:7,...}]`, `run_next` twice with fake runner, assert both commands executed in order without calling planner.

- [ ] **Step 2: Implement `enqueue_flat` + state transitions without `Planning`**

For flat tasks, either skip `CardTaskEvent::Planning` or emit `Planned` immediately.

- [ ] **Step 3: `cargo test -p card_task`**

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Allow card_task to enqueue resolved Create/Focus/SendText steps.

EOF
)"
```

---

### Task 5: App Wiring — Mock Turn + TTS + Template Reports

**Files:**
- Modify: `crates/app/src/app/dialogue/chat.rs`, `card_tools.rs`, `shell.rs`
- Modify: settings if needed for `use_mock_llm` default true in debug
- Tests: `app_dialogue_turn_tests.rs`, `app_dialogue_card_tool_tests.rs`

**Behavior:**

1. ASR final → phase Thinking → background_executor runs `run_voice_turn` with `MockLlmClient` (script from settings or built-in demo script)  
2. Tool executor enqueues each command via `enqueue_flat` **or** executes immediately through the same runner the queue uses — prefer **enqueue_flat one command per tool call** so serial policy holds when multiple tools appear, **or** execute tools inline inside the turn (simpler for multi-round node_id)  

**Important resolution (implement this):**

- For multi-round `node_id`, tool execution during `run_voice_turn` must be **synchronous w.r.t. the turn** (fake/real runner), so Create’s `node_id` returns before the next `llm.chat`.  
- Additionally mirror successful commands into `card_task` history/queue events **or** run the real canvas mutation in the tool executor used by the turn.  
- Recommended MVP: ToolExecutor in app calls the same `CardCommandRunner` that mutates the canvas **immediately**, and also records a flat completed task for UI/metrics if needed. Do **not** wait on the async queue for the LLM-visible tool result.

3. After turn: `on_reply_ready` → TTS synthesize `reply_text` → Speaking → playback  
4. On task/tool success/failure outside reply text: build template string:

| Event | Template (Chinese) |
|-------|--------------------|
| Create ok | `已创建卡片 {node_id}` |
| SendText ok | `已发送到卡片 {node_id}` |
| Focus ok | `已聚焦卡片 {node_id}` |
| Any fail | `操作失败：{message}` |

Pass through `decide_template_report` before speaking.

5. Barge-in still does not cancel tasks.

- [ ] **Step 1: Failing app tests with injected Mock + recording runner**

- [ ] **Step 2: Implement wiring on `background_executor`**

- [ ] **Step 3: `cargo test -p app` relevant filters + `cargo check -p app`**

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Wire Mock LlmClient turns into dialogue chat, tools, and TTS.

EOF
)"
```

---

### Task 6: Ollama `LlmClient` + Listening Readiness Gate

**Files:**
- Create: `crates/llm/src/ollama.rs`
- Modify: settings voice values (endpoint default `http://127.0.0.1:11434`, model default reusable from `voice::OllamaVoicePlanner::default_model()` or a dialogue-specific default like `qwen2.5:7b` — **pick one string and document it in settings UI label**)
- Modify: shell start validation
- Tests: fake TCP/HTTP server tests patterned after `crates/voice/tests/unit/ollama_tests.rs`

**Behavior:**

- `OllamaLlmClient::chat` posts to `/api/chat` with `tools` and parses tool_calls  
- On Always Listening start (non-mock): probe Ollama (`/api/tags` or lightweight chat); on failure set bar error and **do not** enter Listening  
- Invalid tool JSON from model: return tool error result string to model once; if still bad, fail turn with template error TTS  

- [ ] **Step 1: Failing ollama client unit tests with local listener fixture**

- [ ] **Step 2: Implement client + readiness check**

- [ ] **Step 3: Settings toggle Mock vs Ollama for dev**

- [ ] **Step 4: Verify**

```bash
cargo test -p llm -p dialogue -p card_task
cargo check -p app
```

- [ ] **Step 5: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Add Ollama LlmClient and gate Always Listening on readiness.

EOF
)"
```

---

### Task 7: E2E Rewrite And Docs Supersession

**Files:**
- Create: `crates/e2e/tests/local_voice_card_tools_pipeline.rs`
- Delete or gut: `crates/e2e/tests/realtime_card_task_pipeline.rs`
- Modify: `crates/e2e/Cargo.toml` deps (drop `realtime` if still present)
- Modify: `docs/design-docs/realtime-card-task-mvp/README.md` — status: superseded
- Modify: `docs/references/e2e-testing-guide.md` — point to local voice pipeline

**E2E scenario (domain-level, no GPUI required if following current e2e style):**

1. Mock LLM script: Create → SendText with returned node_id  
2. Recording `CardCommandRunner`  
3. `run_voice_turn` + assert commands  
4. Assert template report decision helper for Speaking vs Listening  

Optional ignored test: live Ollama + sherpa models.

- [ ] **Step 1: Write new e2e test (fail until wired)**

- [ ] **Step 2: Remove realtime e2e**

- [ ] **Step 3: Run**

```bash
cargo test -p e2e
cargo test -p audio -p dialogue -p llm -p voice_sherpa -p card_task
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-storage-boundaries.sh
```

- [ ] **Step 4: Commit** (when requested)

```bash
git commit -m "$(cat <<'EOF'
Replace realtime card-task e2e with local voice tool pipeline coverage.

EOF
)"
```

---

### Task 8: Stretch Polish Checklist (Non-Blocking Gate)

Not required to merge functional MVP, but track:

- [ ] Measure ASR-final → TTS-start latency with resident models; record in SPEC acceptance notes  
- [ ] Tune VAD thresholds for low false trigger  
- [ ] Ensure window-close releases sherpa/LLM handles (drop adapters)  
- [ ] Confirm Listening stop cancels unfinished `card_task` entries without unloading models  

Run manual smoke: Always Listening on → speak → hear stub/Ollama reply → barge-in → create+send via voice.

---

## Plan 02 Exit Criteria

- [ ] `LlmClient` Mock + Ollama exist in `crates/llm`
- [ ] ASR final drives Thinking → multi-round tools → reply TTS
- [ ] Create returns `node_id` into the same turn for Focus/SendText
- [ ] Template reports obey queue/defer rules
- [ ] Ollama down ⇒ cannot start Listening (when Mock disabled)
- [ ] Realtime e2e gone; new e2e covers flat tool pipeline
- [ ] Workspace tests for touched crates pass

## Spec Coverage Trace

| SPEC item | Task |
|-----------|------|
| Mock LlmClient | Task 1 |
| Tool schemas Create/Focus/SendText | Task 2 |
| Multi-round node_id | Task 3, 5 |
| Flatten card_task / no Planner on AL path | Task 4 |
| Template TTS reports | Task 5 |
| Ollama + readiness | Task 6 |
| E2E / docs supersession | Task 7 |
| 1–2s stretch / VAD tune | Task 8 |
