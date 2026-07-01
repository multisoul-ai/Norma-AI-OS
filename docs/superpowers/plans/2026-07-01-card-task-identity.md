# Card Task Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display the user's first prompt as a locked task title in each Agent card's status bar, alongside a real-time summary line and elapsed timer, so parallel cards are instantly distinguishable at a glance.

**Architecture:** Add a `task_title: Option<String>` map to `RuntimeEvents` in the `runtime` crate. When a `UserPromptSubmit` hook event arrives, extract the prompt from `raw_json`, truncate to 60 chars, and store it — only on the first occurrence. The `app` layer reads this from `Runtime` to render the status bar row, replacing the current single-badge title bar layout for Agent cards.

**Tech Stack:** Rust, `runtime` crate (no GPUI), `app` crate (GPUI), `norma` crate (hook parsing), `canvas` crate (node model).

## Global Constraints

- `runtime`, `canvas`, `terminal` must remain free of `gpui` imports
- No `#[allow(...)]` attributes — fix root causes
- Single Rust source files ≤ 500 lines
- TDD: write failing tests first, then implement
- Each task ends with a `cargo test` green run and a commit

---

## File Map

| File | Change |
|------|--------|
| `crates/runtime/src/events.rs` | Add `agent_task_titles: HashMap<NodeId, String>` to `RuntimeEvents`; add `agent_task_title()` getter and `set_agent_task_title_if_absent()` writer |
| `crates/runtime/src/lib.rs` | Re-export `agent_task_title()` from `Runtime` |
| `crates/norma/src/agent_hook/event.rs` | Extract prompt text from `UserPromptSubmit` raw_json; add `prompt_text()` helper |
| `crates/app/src/settings/agent_hooks.rs` | Pass prompt to `apply_agent_task_update`; call new `Runtime::set_agent_task_title_if_absent` |
| `crates/runtime/src/events.rs` | Add `AgentTaskUpdate.task_title: Option<String>` field |
| `crates/app/src/cards/agent/card.rs` | Update `chrome()` to use `runtime.agent_task_title(node_id)` as card title when present |
| `crates/app/src/cards/chrome.rs` | Add `status_line: Option<String>` to `CardChrome`; render second row below title bar |
| `crates/app/src/canvas/master_layout.rs` | (No change — card height is fixed; second row fits within existing body area) |
| `crates/runtime/tests/unit/agent_command_tests.rs` | New tests for task title storage and retrieval |
| `crates/app/src/cards/agent/card.rs` (tests) | New test: chrome uses task_title when set; falls back to alias/profile |

---

### Task 1: Extend `AgentTaskUpdate` with optional task title and store it in `RuntimeEvents`

**Files:**
- Modify: `crates/runtime/src/events.rs`
- Modify: `crates/runtime/tests/unit/agent_command_tests.rs`

**Interfaces:**
- Produces:
  - `AgentTaskUpdate { ..., task_title: Option<String> }` — new field, `None` by default
  - `RuntimeEvents::agent_task_title(node_id: NodeId) -> Option<String>`
  - `RuntimeEvents::set_agent_task_title_if_absent(node_id: NodeId, title: String)` — no-op if already set
  - `Runtime::agent_task_title(node_id: NodeId) -> Option<String>` — public re-export

- [ ] **Step 1: Write failing tests**

Add to `crates/runtime/tests/unit/agent_command_tests.rs`:

```rust
/// T-title-1: 首次 UserPromptSubmit 将 task_title 写入 runtime，后续调用不覆盖。
///
/// 数据构造：
///   node_id = 1, profile = Claude
///   第一次 apply_agent_task_update task_title = Some("帮我重构")
///   第二次 apply_agent_task_update task_title = Some("现在再做别的")
///
/// 执行过程：
///   1. 两次 apply_agent_task_update，task_title 分别为 Some("帮我重构") 和 Some("现在再做别的")
///   2. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 为 "帮我重构"（首次写入，锁定）
///   - 负断言：不应为 "现在再做别的"（第二次不应覆盖）
#[test]
fn test_task_title_locked_on_first_set() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("帮我重构".to_string()),
    });
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("现在再做别的".to_string()),
    });

    assert_eq!(
        runtime.agent_task_title(node_id),
        Some("帮我重构".to_string()),
        "task_title 应在首次写入后锁定，不被第二次覆盖"
    );
    assert_ne!(
        runtime.agent_task_title(node_id),
        Some("现在再做别的".to_string()),
        "第二次 task_title 不应覆盖首次写入的值"
    );
}

/// T-title-2: task_title 为 None 时不写入，agent_task_title 返回 None。
///
/// 数据构造：
///   node_id = 1, profile = Codex
///   apply_agent_task_update task_title = None
///
/// 执行过程：
///   1. apply_agent_task_update task_title = None
///   2. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 返回 None
#[test]
fn test_task_title_none_does_not_write() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Codex,
            alias: None,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );

    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Codex,
        provider_event: "PreToolUse".to_string(),
        to: runtime::AgentTaskState::RunningTool { tool_name: Some("bash".to_string()) },
        task_title: None,
    });

    assert_eq!(
        runtime.agent_task_title(node_id),
        None,
        "task_title=None 时不应写入任何值"
    );
}

/// T-title-3: 节点被移除后，task_title 也应清除。
///
/// 数据构造：
///   node_id = 1, profile = Claude, task_title = "test task"
///
/// 执行过程：
///   1. 写入 task_title
///   2. runtime.remove_node(node_id)
///   3. 读取 runtime.agent_task_title(node_id)
///
/// 预期结果：
///   - 正断言：agent_task_title 返回 None
#[test]
fn test_task_title_cleared_on_node_remove() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        canvas::CanvasNodeKind::Agent(canvas::AgentSpec {
            profile: canvas::AgentProfile::Claude,
            alias: None,
        }),
        canvas::CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    runtime.register_node(node_id);

    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: canvas::AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("test task".to_string()),
    });

    runtime.remove_node(node_id);

    assert_eq!(
        runtime.agent_task_title(node_id),
        None,
        "节点移除后 task_title 应被清除"
    );
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p runtime 2>&1 | grep -E "FAILED|error\[" | head -20
```

Expected: compile errors — `task_title` field missing on `AgentTaskUpdate`, `agent_task_title` method missing on `Runtime`.

- [ ] **Step 3: Add `task_title` field to `AgentTaskUpdate`**

In `crates/runtime/src/events.rs`, find `AgentTaskUpdate` (line ~43) and add field:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTaskUpdate {
    pub node_id: NodeId,
    pub profile: AgentProfile,
    pub provider_event: String,
    pub to: AgentTaskState,
    pub task_title: Option<String>,  // add this line
}
```

- [ ] **Step 4: Add `agent_task_titles` map and methods to `RuntimeEvents`**

In `crates/runtime/src/events.rs`, inside `RuntimeEvents` struct (line ~82), add field:

```rust
pub(crate) struct RuntimeEvents {
    states: HashMap<NodeId, NodeRunState>,
    agent_task_states: HashMap<NodeId, AgentTaskState>,
    agent_task_titles: HashMap<NodeId, String>,          // add this line
    card_attention: HashMap<NodeId, CardAttentionState>,
    queue: VecDeque<RuntimeEvent>,
}
```

After `agent_task_state()` getter (line ~95), add:

```rust
pub(crate) fn agent_task_title(&self, node_id: NodeId) -> Option<String> {
    self.agent_task_titles.get(&node_id).cloned()
}

pub(crate) fn set_agent_task_title_if_absent(&mut self, node_id: NodeId, title: String) {
    self.agent_task_titles.entry(node_id).or_insert(title);
}
```

In the `remove()` method (line ~133), add cleanup:

```rust
pub(crate) fn remove(&mut self, node_id: NodeId) {
    self.agent_task_states.remove(&node_id);
    self.agent_task_titles.remove(&node_id);    // add this line
    self.card_attention.remove(&node_id);
    // ... rest unchanged
}
```

In the `remove_all()` method (line ~144), add cleanup:

```rust
pub(crate) fn remove_all(&mut self) {
    self.agent_task_states.clear();
    self.agent_task_titles.clear();             // add this line
    self.card_attention.clear();
    // ... rest unchanged
}
```

- [ ] **Step 5: Call `set_agent_task_title_if_absent` from `update_agent_task_state`**

In `update_agent_task_state` — but this method doesn't receive `task_title`. We need to pass it through `apply_agent_task_update` on `Runtime`. In `crates/runtime/src/events.rs`, update `apply_agent_task_update` (line ~263):

```rust
pub fn apply_agent_task_update(&mut self, update: AgentTaskUpdate) -> bool {
    let Some(node) = self.document.node(update.node_id) else {
        tracing::debug!(
            node_id = update.node_id,
            "agent task update ignored: missing node"
        );
        return false;
    };
    match &node.kind {
        CanvasNodeKind::Agent(agent) if agent.profile == update.profile => {
            // write task_title on first UserPromptSubmit
            if let Some(title) = update.task_title {
                self.events.set_agent_task_title_if_absent(update.node_id, title);
            }
            self.events
                .update_agent_task_state(update.node_id, update.to, update.provider_event)
        }
        // ... rest unchanged
    }
}
```

- [ ] **Step 6: Add public getter on `Runtime`**

In `crates/runtime/src/events.rs`, after `agent_task_state()` public getter (line ~248), add:

```rust
pub fn agent_task_title(&self, node_id: NodeId) -> Option<String> {
    self.events.agent_task_title(node_id)
}
```

- [ ] **Step 7: Re-export from `runtime::lib.rs`**

In `crates/runtime/src/lib.rs`, find the line that re-exports `agent_task_state` and check it's already exported via the `Runtime` struct (it is — it's a method, no extra re-export needed). Verify the method is visible:

```bash
grep -n "pub fn agent_task_title\|pub use" crates/runtime/src/events.rs crates/runtime/src/lib.rs
```

- [ ] **Step 8: Fix all call sites that construct `AgentTaskUpdate`**

Search for all `AgentTaskUpdate {` usages and add `task_title: None`:

```bash
grep -rn "AgentTaskUpdate {" crates/ --include="*.rs"
```

Update each existing construction to include `task_title: None` (e.g. in `agent_hooks.rs`, existing tests).

- [ ] **Step 9: Run tests**

```bash
cargo test -p runtime 2>&1 | tail -20
```

Expected: all existing tests pass, new T-title-* tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/runtime/src/events.rs crates/runtime/tests/unit/agent_command_tests.rs
git commit -m "feat(runtime): add task_title to AgentTaskUpdate; lock on first UserPromptSubmit"
```

---

### Task 2: Extract prompt text from `UserPromptSubmit` hook event in `norma`

**Files:**
- Modify: `crates/norma/src/agent_hook/event.rs`
- Modify: `crates/app/src/settings/agent_hooks.rs`

**Interfaces:**
- Consumes: `AgentHookEvent { provider_event, raw_json, .. }` from Task 1's context
- Produces:
  - `extract_prompt_text(raw_json: &Value) -> Option<String>` — tries keys `["prompt", "message", "user_message"]`, truncates to 60 chars + `…`
  - `apply_agent_hook_event` passes `task_title` to `AgentTaskUpdate`

- [ ] **Step 1: Write failing test in norma**

Create `crates/norma/tests/unit/` directory if needed, or add to existing test file. Find existing test location:

```bash
find crates/norma -name "*.rs" | xargs grep -l "#\[test\]" 2>/dev/null
```

Add tests in `crates/norma/src/agent_hook/event.rs` (in existing `#[cfg(test)]` block or add one):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// extract_prompt_text: UserPromptSubmit raw_json 中有 "prompt" 字段时返回截断后的文本。
    ///
    /// 数据构造：
    ///   raw_json = {"prompt": "帮我重构 user_service.rs 的错误处理逻辑"}（26字，< 60）
    ///   raw_json_long: prompt 为 70 字的字符串
    ///
    /// 执行过程：
    ///   1. 调用 extract_prompt_text(&raw_json)
    ///   2. 调用 extract_prompt_text(&raw_json_long)
    ///
    /// 预期结果：
    ///   - 正断言：短 prompt 原样返回
    ///   - 正断言：长 prompt 截断到 60 字 + "…"
    ///   - 负断言：长 prompt 不应返回完整 70 字
    #[test]
    fn test_extract_prompt_text_short_and_truncated() {
        let short = json!({"hook_event_name": "UserPromptSubmit", "prompt": "帮我重构 user_service.rs"});
        let long_text = "A".repeat(70);
        let long = json!({"hook_event_name": "UserPromptSubmit", "prompt": long_text});

        let result_short = extract_prompt_text(&short);
        let result_long = extract_prompt_text(&long);

        assert_eq!(
            result_short,
            Some("帮我重构 user_service.rs".to_string()),
            "短 prompt 应原样返回"
        );
        let long_result = result_long.expect("长 prompt 应返回 Some");
        assert!(
            long_result.ends_with('…'),
            "长 prompt 应以 … 结尾，实际：{long_result}"
        );
        assert!(
            long_result.chars().count() <= 61,
            "截断后长度应 ≤ 61（60字 + …），实际：{}",
            long_result.chars().count()
        );
        assert_ne!(
            long_result.chars().count(),
            70,
            "长 prompt 不应原样返回完整 70 字"
        );
    }

    /// extract_prompt_text: "message" 字段作为备选键。
    ///
    /// 数据构造：
    ///   raw_json = {"hook_event_name": "UserPromptSubmit", "message": "hello"}
    ///   raw_json_no_keys = {"hook_event_name": "UserPromptSubmit"}
    ///
    /// 预期结果：
    ///   - 正断言：有 "message" 时返回 Some("hello")
    ///   - 正断言：无任何已知键时返回 None
    #[test]
    fn test_extract_prompt_text_fallback_keys_and_none() {
        let with_message = json!({"hook_event_name": "UserPromptSubmit", "message": "hello"});
        let no_keys = json!({"hook_event_name": "UserPromptSubmit"});

        assert_eq!(
            extract_prompt_text(&with_message),
            Some("hello".to_string()),
            "有 message 字段时应返回其值"
        );
        assert_eq!(
            extract_prompt_text(&no_keys),
            None,
            "无已知 prompt 键时应返回 None"
        );
    }
}
```

- [ ] **Step 2: Run test to confirm compile error**

```bash
cargo test -p norma 2>&1 | grep -E "error\[|FAILED" | head -10
```

Expected: `extract_prompt_text` not found.

- [ ] **Step 3: Implement `extract_prompt_text` in `event.rs`**

In `crates/norma/src/agent_hook/event.rs`, add after the existing `string_field` helper:

```rust
/// Extract user prompt text from a UserPromptSubmit raw_json.
/// Tries keys in order: "prompt", "message", "user_message".
/// Truncates to 60 chars (by char boundary) and appends "…" if longer.
pub fn extract_prompt_text(raw_json: &Value) -> Option<String> {
    let text = ["prompt", "message", "user_message"]
        .iter()
        .find_map(|key| string_field(raw_json, key))?;
    if text.chars().count() <= 60 {
        Some(text)
    } else {
        let truncated: String = text.chars().take(60).collect();
        Some(format!("{truncated}…"))
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p norma 2>&1 | tail -10
```

Expected: all norma tests pass.

- [ ] **Step 5: Wire `extract_prompt_text` into `apply_agent_hook_event`**

In `crates/app/src/settings/agent_hooks.rs`, update `apply_agent_hook_event` (line ~82):

```rust
pub(crate) fn apply_agent_hook_event(runtime: &mut Runtime, event: AgentHookEvent) -> bool {
    let Some(to) = agent_task_state_from_hook_event(&event) else {
        tracing::debug!(provider_event = %event.provider_event, "ignored unmapped agent hook event");
        return false;
    };
    let task_title = if event.provider_event == "UserPromptSubmit" {
        norma::agent_hook::extract_prompt_text(&event.raw_json)
    } else {
        None
    };
    runtime.apply_agent_task_update(AgentTaskUpdate {
        node_id: event.node_id,
        profile: canvas_profile(event.profile),
        provider_event: event.provider_event,
        to,
        task_title,
    })
}
```

Add the import at the top of the file if `norma::agent_hook::extract_prompt_text` is not already in scope:

```rust
use norma::agent_hook::{extract_prompt_text, task_state_for_provider_event, AgentHookEvent, AgentHookTaskState};
```

- [ ] **Step 6: Run workspace tests**

```bash
cargo test -p runtime -p norma -p app 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/norma/src/agent_hook/event.rs crates/app/src/settings/agent_hooks.rs
git commit -m "feat(norma): extract prompt text from UserPromptSubmit; wire into AgentTaskUpdate"
```

---

### Task 3: Update `AgentCard.chrome()` to use `task_title`, and add `status_line` to `CardChrome`

**Files:**
- Modify: `crates/app/src/cards/chrome.rs`
- Modify: `crates/app/src/cards/agent/card.rs`
- Modify: `crates/app/src/cards/term/card.rs` (add `status_line: None`)
- Modify: `crates/app/src/cards/preview/` (add `status_line: None` to any `CardChrome` constructions)
- Modify: `crates/app/src/cards/chrome.rs` (add `EmptyCard` status_line: None)

**Interfaces:**
- Consumes: `Runtime::agent_task_title(node_id)` from Task 1
- Consumes: `Runtime::agent_task_state(node_id)` (already exists)
- Produces:
  - `CardChrome { title, refreshable, task_badge, status_line: Option<String> }`
  - `agent_status_line(state: Option<AgentTaskState>) -> Option<String>` — pure function, easily testable

- [ ] **Step 1: Write failing tests in `agent/card.rs`**

Add to the `#[cfg(test)]` block at the bottom of `crates/app/src/cards/agent/card.rs`:

```rust
/// AgentCard chrome 在 runtime 有 task_title 时应使用它作为标题，而非 profile 名。
///
/// 数据构造：
///   node_id=1, profile=Claude, alias=None
///   runtime 写入 task_title = "帮我重构 user_service.rs"（首次 UserPromptSubmit 触发）
///
/// 执行过程：
///   1. apply_agent_task_update task_title=Some("帮我重构 user_service.rs"), to=Thinking
///   2. 调用 AgentCard.chrome()
///
/// 预期结果：
///   - 正断言：chrome.title = "帮我重构 user_service.rs"
///   - 负断言：chrome.title ≠ "Claude"（task_title 应覆盖 profile 名）
#[test]
fn test_agent_card_chrome_uses_task_title_when_set() {
    let mut runtime = Runtime::new();
    let node_id = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Claude,
            alias: None,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    runtime.apply_agent_task_update(runtime::AgentTaskUpdate {
        node_id,
        profile: AgentProfile::Claude,
        provider_event: "UserPromptSubmit".to_string(),
        to: runtime::AgentTaskState::Thinking,
        task_title: Some("帮我重构 user_service.rs".to_string()),
    });
    let node = runtime.document.node(node_id).unwrap().clone();

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(
        chrome.title, "帮我重构 user_service.rs",
        "task_title 存在时 chrome.title 应使用 task_title"
    );
    assert_ne!(
        chrome.title, "Claude",
        "task_title 存在时不应回退到 profile 名"
    );
}

/// AgentCard chrome: task_title 未设置时，标题回退到 alias（若有）或 profile 名。
///
/// 数据构造：
///   node_id=2, profile=Codex, alias=None；未发过 UserPromptSubmit
///
/// 执行过程：
///   1. 不调用 apply_agent_task_update（无 task_title）
///   2. 调用 AgentCard.chrome()
///
/// 预期结果：
///   - 正断言：chrome.title = "Codex"（回退到 profile 名）
#[test]
fn test_agent_card_chrome_falls_back_to_profile_when_no_task_title() {
    let runtime = Runtime::new();
    let node = CanvasNode {
        id: 99,
        kind: CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
        }),
        bounds: CanvasRect::new(0.0, 0.0, 100.0, 80.0),
        z_index: 1,
        selected: false,
    };

    let chrome = CARD.chrome(&node, &runtime);

    assert_eq!(chrome.title, "Codex", "无 task_title 时应回退到 profile 名");
}

/// agent_status_line: 各 AgentTaskState 返回正确的摘要文本。
///
/// 执行过程：
///   对每种状态调用 agent_status_line()，检查返回值
///
/// 预期结果：
///   - Thinking → Some("Thinking…")
///   - RunningTool Some("bash") → Some("Running: bash")
///   - RunningTool None → Some("Running tool…")
///   - WaitingApproval Some("Bash") → Some("Waiting approval: Bash")
///   - Done → Some("Completed")
///   - Failed "boom" → Some("Failed: boom")
///   - Ready → Some("Waiting for input")
///   - None → None
#[test]
fn test_agent_status_line_maps_all_states() {
    use runtime::AgentTaskState;
    assert_eq!(agent_status_line(Some(AgentTaskState::Thinking)), Some("Thinking…".to_string()));
    assert_eq!(
        agent_status_line(Some(AgentTaskState::RunningTool { tool_name: Some("bash".to_string()) })),
        Some("Running: bash".to_string())
    );
    assert_eq!(
        agent_status_line(Some(AgentTaskState::RunningTool { tool_name: None })),
        Some("Running tool…".to_string())
    );
    assert_eq!(
        agent_status_line(Some(AgentTaskState::WaitingApproval { tool_name: Some("Bash".to_string()) })),
        Some("Waiting approval: Bash".to_string())
    );
    assert_eq!(agent_status_line(Some(AgentTaskState::Done)), Some("Completed".to_string()));
    assert_eq!(
        agent_status_line(Some(AgentTaskState::Failed { message: "boom".to_string() })),
        Some("Failed: boom".to_string())
    );
    assert_eq!(agent_status_line(Some(AgentTaskState::Ready)), Some("Waiting for input".to_string()));
    assert_eq!(agent_status_line(None), None);
}
```

- [ ] **Step 2: Run tests to confirm failures**

```bash
cargo test -p app -- cards::agent 2>&1 | grep -E "FAILED|error\[" | head -10
```

Expected: `agent_status_line` not found, `task_title` field not found on `AgentTaskUpdate`, compile errors.

- [ ] **Step 3: Add `status_line` field to `CardChrome`**

In `crates/app/src/cards/chrome.rs`, update `CardChrome` (line ~21):

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardChrome {
    pub title: String,
    pub refreshable: bool,
    pub task_badge: Option<CardChromeTaskBadge>,
    pub status_line: Option<String>,   // add this line
}
```

Fix all `CardChrome { ... }` constructions to include `status_line: None`:

- In `chrome.rs` `EmptyCard::chrome` (line ~286): add `status_line: None`
- In `agent/card.rs` `AgentCard::chrome` (line ~32): will be updated in next step
- In any other `CardChrome` construction: add `status_line: None`

Find all usages:

```bash
grep -rn "CardChrome {" crates/app/src/ --include="*.rs"
```

- [ ] **Step 4: Add `agent_status_line` function and update `AgentCard::chrome`**

In `crates/app/src/cards/agent/card.rs`, add below `agent_task_badge`:

```rust
pub(crate) fn agent_status_line(state: Option<AgentTaskState>) -> Option<String> {
    Some(match state? {
        AgentTaskState::Ready => "Waiting for input".to_string(),
        AgentTaskState::Thinking => "Thinking…".to_string(),
        AgentTaskState::RunningTool { tool_name: Some(name) } => format!("Running: {name}"),
        AgentTaskState::RunningTool { tool_name: None } => "Running tool…".to_string(),
        AgentTaskState::WaitingApproval { tool_name: Some(name) } => format!("Waiting approval: {name}"),
        AgentTaskState::WaitingApproval { tool_name: None } => "Waiting approval…".to_string(),
        AgentTaskState::Done => "Completed".to_string(),
        AgentTaskState::Failed { message } => format!("Failed: {message}"),
        AgentTaskState::Ended => "Session ended".to_string(),
    })
}
```

Update `AgentCard::chrome` (line ~23) to use `task_title` and `status_line`:

```rust
fn chrome(&self, node: &CanvasNode, runtime: &Runtime) -> CardChrome {
    // Use task_title (locked on first UserPromptSubmit) if available;
    // fall back to alias then profile name.
    let title = runtime
        .agent_task_title(node.id)
        .or_else(|| match &node.kind {
            CanvasNodeKind::Agent(agent) => agent.alias.clone(),
            _ => None,
        })
        .unwrap_or_else(|| match &node.kind {
            CanvasNodeKind::Agent(agent) => agent.profile.display_title().to_string(),
            _ => "Agent".to_string(),
        });
    let state = runtime.agent_task_state(node.id);
    CardChrome {
        title,
        refreshable: false,
        task_badge: agent_task_badge(state.clone()),
        status_line: agent_status_line(state),
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p app -- cards::agent 2>&1 | tail -20
```

Expected: all agent card tests pass.

- [ ] **Step 6: Run full workspace tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Fix any compile errors from `CardChrome` missing `status_line` field at other call sites.

- [ ] **Step 7: Commit**

```bash
git add crates/app/src/cards/chrome.rs crates/app/src/cards/agent/card.rs
git commit -m "feat(app): add status_line to CardChrome; AgentCard chrome uses task_title and state summary"
```

---

### Task 4: Render status_line as a second row below the title bar in `canvas/view.rs`

**Files:**
- Modify: `crates/app/src/canvas/view.rs`

**Interfaces:**
- Consumes: `chrome.status_line: Option<String>` from Task 3
- No new public API — this is purely a rendering change

Note: The status line is rendered as a slim secondary bar immediately below the title bar, only when `status_line` is `Some`. It does NOT affect layout height (cards have a fixed height determined by `CanvasRect`); it overlaps the top of the body area, visually reading as a two-row header.

- [ ] **Step 1: Locate the title bar render block in `canvas/view.rs`**

The title bar is rendered at line ~421. The block ends at line ~453 (`.child(title_actions)`). The content body starts at line ~456.

- [ ] **Step 2: Extract `status_line` from chrome alongside `task_badge`**

In `view.rs`, at line ~286, add `status_line` to the destructuring:

```rust
let chrome = card.chrome(&node, &app.runtime);
let (origin, size, selected, title, task_badge, status_line) = (
    node.bounds.origin,
    node.bounds.size,
    node.selected,
    chrome.title,
    chrome.task_badge,
    chrome.status_line,   // add this
);
```

- [ ] **Step 3: Render the status line row after the title bar child**

After the title bar `.child(...)` block (after line ~453), add a conditional second row:

```rust
// Status line (Agent task summary row)
if let Some(ref line) = status_line {
    let status_h = 18.0 * vp.zoom;
    node_div = node_div.child(
        div()
            .h(px(status_h))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .px(px(10.0 * vp.zoom))
            .bg(rgba(with_alpha(visual.title_bar_color, 0xcc)))
            .text_color(rgba(theme.text_secondary))
            .text_size(px((10.0 * vp.zoom).max(8.0)))
            .child(line.clone()),
    );
}
```

- [ ] **Step 4: Check file line count**

```bash
wc -l crates/app/src/canvas/view.rs
```

If over 500 lines, flag for a split. The current file is expected to be under 500; this adds ~10 lines.

- [ ] **Step 5: Run the app and verify visually**

```bash
cargo run -p app
```

Expected: Agent cards now show a slim status row below the title bar when a task is running. Terminal cards show no second row.

- [ ] **Step 6: Run workspace tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/app/src/canvas/view.rs
git commit -m "feat(app): render agent task status line below title bar in canvas view"
```

---

## Self-Review Against Spec

| Spec Requirement | Task |
|-----------------|------|
| 状态点颜色映射（Thinking=蓝/RunningTool=黄/etc） | Existing `agent_task_badge` already provides this via `CardChromeTaskTone`; no change needed |
| 任务标题：第一条 UserPromptSubmit prompt 锁定 | Task 1 + Task 2 |
| 摘要行：实时映射当前 AgentTaskState | Task 3 (`agent_status_line`) |
| 摘要行渲染 | Task 4 |
| Prompt 超 60 字截断 + … | Task 2 (`extract_prompt_text`) |
| 无 prompt 时回退到 alias/profile | Task 3 (chrome fallback chain) |
| 节点移除后清理 task_title | Task 1 (`remove()` cleanup) |
| Terminal 卡片显示当前命令（已有，不变） | Out of scope — Terminal already renders running command in body |
| 运行时长显示 | **Not implemented** — SPEC lists this as a requirement; see below |

### Gap: 运行时长

SPEC §4.2 requires an elapsed timer in the title bar. This was discussed but not included in the tasks above to keep scope small. It requires:
- Recording `registered_at: Instant` per node in `RuntimeEvents` (or app layer)
- A GPUI `cx.spawn` timer in `view.rs` to drive 1s repaint

This can be added as Task 5 in a follow-up plan, or the user can confirm scope before implementation.
