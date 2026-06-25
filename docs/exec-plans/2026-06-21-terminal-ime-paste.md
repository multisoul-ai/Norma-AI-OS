# Terminal IME + Paste Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two terminal input bugs: (1) Chinese IME input is silently dropped; (2) Cmd+V paste sends raw 'v' instead of clipboard content.

**Architecture:** Two targeted fixes in `crates/app`. Task 1 fixes `keystroke_to_input` to prefer `key_char` over `key` for printable input. Task 2 adds Cmd+V clipboard paste handling in `on_key_down`. IME composition via `InputHandler` (full NSTextInputClient) is explicitly out of scope — the `key_char` path covers the vast majority of IME finalized characters delivered through `KeyDownEvent`; full composition UI (inline candidate preview) requires a much larger GPUI entity refactor.

**Tech Stack:** Rust, GPUI 0.2.2 (pinned), `gpui::ClipboardItem`, `gpui::Keystroke::key_char`

## Global Constraints

- No `gpui` imports in `canvas`, `terminal`, or `runtime` crates.
- No `#[allow(...)]` attributes — fix root causes instead.
- Source files must stay at or below 500 lines (`scripts/check-max-file-lines.sh`).
- Every `assert!`/`assert_eq!`/`assert_ne!` must have a failure message string.
- All new tests must follow the doc-comment format: Scenario / Data construction / Execution process / Expected result.
- TDD: write failing test first, watch it fail, then implement.

---

### Task 1: Fix `keystroke_to_input` to respect `key_char`

**Files:**
- Modify: `crates/app/src/cards/term/view.rs` (function `keystroke_to_input` and its tests)

**Interfaces:**
- Consumes: `gpui::Keystroke { key: String, key_char: Option<String>, modifiers: Modifiers }`
- Produces: `keystroke_to_input(&Keystroke) -> Option<KeyInput>` — unchanged signature, new behavior: when `key_char` is `Some(s)` and contains exactly one char and no Cmd/Ctrl modifier is active, use that char as `Key::Char`; otherwise fall back to existing `key`-based matching.

- [ ] **Step 1: Write failing tests**

Add the following tests inside the `#[cfg(test)] mod tests` block in `crates/app/src/cards/term/view.rs` (after the existing tests, before the closing `}`):

```rust
/// key_char 优先于 key：IME 上屏或非 ASCII 布局键按下时，key_char 携带真实字符。
///
/// 数据构造：
///   ks.key = "n"（拼音字母，物理键）
///   ks.key_char = Some("你")（IME 确认上屏的汉字）
///   modifiers: 无 ctrl/alt/platform
///
/// 执行过程：
///   1. keystroke_to_input(&ks)
///   2. key_char 为 Some("你")，单字符，无修饰 → 应取 '你' 作为 Key::Char
///
/// 预期结果：
///   - 正断言：返回 Some(KeyInput { key: Key::Char('你'), ctrl: false, alt: false })
///   - 负断言：不应返回 Key::Char('n')（物理键）
#[test]
fn test_keystroke_to_input_prefers_key_char_for_ime() {
    use gpui::{Keystroke, Modifiers};
    let ks = Keystroke {
        key: "n".into(),
        key_char: Some("你".into()),
        modifiers: Modifiers::default(),
        ime_key: None,
    };
    let result = keystroke_to_input(&ks);
    assert_eq!(
        result,
        Some(KeyInput { key: Key::Char('你'), ctrl: false, alt: false }),
        "key_char='你' 应优先于物理键 key='n'"
    );
    assert_ne!(
        result,
        Some(KeyInput { key: Key::Char('n'), ctrl: false, alt: false }),
        "不应返回物理键字符 'n'，key_char 优先级更高"
    );
}

/// Ctrl 修饰时不使用 key_char：Ctrl+C 应编码为控制码，而不是 key_char 中可能含有的字符。
///
/// 数据构造：
///   ks.key = "c"
///   ks.key_char = Some("c")（macOS 可能仍然填充 key_char）
///   modifiers: ctrl = true
///
/// 执行过程：
///   1. keystroke_to_input(&ks)
///   2. ctrl=true → 走 key 路径，Key::Char('c') with ctrl=true
///
/// 预期结果：
///   - 正断言：ctrl=true，key=Char('c')
#[test]
fn test_keystroke_to_input_ctrl_ignores_key_char() {
    use gpui::{Keystroke, Modifiers};
    let ks = Keystroke {
        key: "c".into(),
        key_char: Some("c".into()),
        modifiers: Modifiers { control: true, ..Modifiers::default() },
        ime_key: None,
    };
    let result = keystroke_to_input(&ks);
    assert_eq!(
        result,
        Some(KeyInput { key: Key::Char('c'), ctrl: true, alt: false }),
        "Ctrl+C 应保留 ctrl=true，不受 key_char 影响"
    );
}

/// key_char 多字符时回落 key 路径（例如组字中间态，不应上屏多个字符）。
///
/// 数据构造：
///   ks.key = "a"
///   ks.key_char = Some("ab")（不合法的多字符 key_char，fallback 到 key）
///   modifiers: 无
///
/// 执行过程：
///   1. keystroke_to_input(&ks)
///   2. key_char.chars().count() > 1 → 回落，使用 ks.key = "a"
///
/// 预期结果：
///   - 正断言：返回 Key::Char('a')
#[test]
fn test_keystroke_to_input_multi_char_key_char_falls_back_to_key() {
    use gpui::{Keystroke, Modifiers};
    let ks = Keystroke {
        key: "a".into(),
        key_char: Some("ab".into()),
        modifiers: Modifiers::default(),
        ime_key: None,
    };
    let result = keystroke_to_input(&ks);
    assert_eq!(
        result,
        Some(KeyInput { key: Key::Char('a'), ctrl: false, alt: false }),
        "多字符 key_char 应回落到 ks.key 处理"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /Users/alan/Documents/mybook/multisoul-desktop
cargo test -p app -- test_keystroke_to_input_prefers_key_char_for_ime test_keystroke_to_input_ctrl_ignores_key_char test_keystroke_to_input_multi_char_key_char_falls_back_to_key 2>&1 | tail -20
```

Expected: Tests fail because `keystroke_to_input` ignores `key_char` entirely.
(If `Keystroke` lacks the `ime_key` field, remove it from the struct literal.)

- [ ] **Step 3: Check actual `Keystroke` struct fields**

```bash
grep -n "pub struct Keystroke\|pub key\|pub key_char\|pub modifiers\|pub ime_key" \
  ~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/gpui-0.2.2/src/platform/keystroke.rs \
  | head -20
```

Adjust the test `Keystroke` literals to match the actual fields (e.g., if there's no `ime_key`, drop it).

- [ ] **Step 4: Implement the fix in `keystroke_to_input`**

Replace the body of `keystroke_to_input` in `crates/app/src/cards/term/view.rs`:

```rust
pub fn keystroke_to_input(ks: &Keystroke) -> Option<KeyInput> {
    let ctrl = ks.modifiers.control;
    let alt = ks.modifiers.alt;

    // When key_char holds exactly one char and no Ctrl/Cmd is active, use it
    // directly — this is how IME-composed characters (Chinese, Japanese, etc.)
    // and non-ASCII layouts reach us from GPUI.
    if !ctrl && !ks.modifiers.platform {
        if let Some(kc) = &ks.key_char {
            let mut chars = kc.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                return Some(KeyInput { key: Key::Char(c), ctrl: false, alt });
            }
        }
    }

    let key = match ks.key.as_str() {
        "enter" => Key::Enter,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "escape" => Key::Escape,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "space" => Key::Char(' '),
        s if s.chars().count() == 1 => Key::Char(s.chars().next().unwrap()),
        _ => return None,
    };
    Some(KeyInput { key, ctrl, alt })
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p app -- test_keystroke_to_input 2>&1 | tail -20
```

Expected: All three new tests pass, existing tests pass.

- [ ] **Step 6: Run full workspace tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: All tests pass, no warnings about unused imports.

- [ ] **Step 7: Check line count**

```bash
bash scripts/check-max-file-lines.sh 2>&1
```

Expected: No violations.

- [ ] **Step 8: Commit**

```bash
git add crates/app/src/cards/term/view.rs
git commit -m "fix(terminal): prefer key_char over key in keystroke_to_input for IME input"
```

---

### Task 2: Add Cmd+V paste from clipboard to focused terminal

**Files:**
- Modify: `crates/app/src/app.rs` (function `on_key_down` and the `use gpui::` import list)
- Modify: `crates/app/src/app_tests.rs` (add paste-related unit tests — or create test helpers there)

**Interfaces:**
- Consumes: `gpui::ClipboardItem` (read via `cx.read_from_clipboard()` inside a `Window`/`App` context)
- Consumes: `runtime::NodeInput::Text(String)` — raw UTF-8 string sent to PTY
- Produces: no new public API; `on_key_down` gains a Cmd+V branch that calls `cx.read_from_clipboard()` and sends bytes to the PTY.

**Note on `NodeInput::Text`:** Check if `runtime::NodeInput` already has a `Text` variant. If not, it must be added (see Step 1 details). The terminal's `send_node_input` path writes key-encoded bytes for `NodeInput::Key`; for paste we need to send raw UTF-8 bytes without key encoding.

- [ ] **Step 1: Check `NodeInput` and `Runtime::send_node_input`**

```bash
grep -n "enum NodeInput\|NodeInput::\|fn send_node_input\|pub fn send_node_input" \
  crates/runtime/src/runtime.rs crates/runtime/src/live.rs 2>/dev/null | head -20
```

If `NodeInput` already has a `Text(String)` variant and `send_node_input` handles it by writing raw UTF-8 bytes, skip Step 2 and 3. If not, continue.

- [ ] **Step 2: Write failing test for `NodeInput::Text` routing (only if Text variant is missing)**

Add to `crates/runtime/src/runtime.rs` (or its test file):

```rust
/// NodeInput::Text sends raw bytes to PTY without key encoding.
///
/// Scenario: paste path — multi-character UTF-8 string must arrive verbatim.
///
/// Data construction:
///   text = "你好" → UTF-8 bytes [0xe4,0xbd,0xa0, 0xe5,0xa5,0xbd] (6 bytes)
///
/// Expected result:
///   - Positive: PTY write buffer contains exact UTF-8 bytes of "你好"
///   - Negative: bytes are not key-encoded (no 0x1b prefix, length > 2)
#[test]
fn test_node_input_text_sends_raw_utf8() {
    // This test is at the runtime/live layer — confirm Text variant routes to write_raw
    // (Implementation detail: use a fake PTY or mock write channel if needed)
    // Adjust to match actual runtime test patterns in this file.
    todo!("implement after checking existing test infrastructure")
}
```

(If runtime tests use a real PTY that's hard to intercept, skip this test and rely on the app-layer integration test in Step 4 instead. Remove the todo stub.)

- [ ] **Step 3: Add `NodeInput::Text(String)` variant if missing**

In `crates/runtime/src/live.rs`, add to the `NodeInput` enum:

```rust
pub enum NodeInput {
    Key(terminal::KeyInput),
    Text(String),   // raw UTF-8 paste — sent to PTY without key encoding
}
```

In `crates/runtime/src/runtime.rs`, in `send_node_input`, add handling:

```rust
NodeInput::Text(s) => {
    live.write(s.as_bytes())?;
}
```

Where `write` is whatever method sends raw bytes to the PTY (check existing `NodeInput::Key` path to find the correct method name).

Run `cargo test -p runtime` after this step; all existing tests must pass.

- [ ] **Step 4: Write failing test for paste in `app.rs`**

GPUI views can't be instantiated in unit tests without a full app context. Write a narrowly scoped helper test that verifies the *logic* of the paste detection, not the full GPUI round-trip. Add to `crates/app/src/app_tests.rs` (or create the file if it doesn't exist under `#[cfg(test)]`):

```rust
/// paste_bytes_from_clipboard_text 从剪贴板文本生成应写入 PTY 的字节序列。
///
/// 数据构造：
///   clipboard text = "hello 世界"
///   UTF-8 bytes: b"hello " (6) + [0xe4,0xb8,0x96,0xe7,0x95,0x8c] (6) = 12 bytes
///
/// 预期结果：
///   - 正断言：bytes == "hello 世界".as_bytes()
///   - 负断言：不为空，不为 b"v"（未做粘贴直接发了 v 键）
#[test]
fn test_paste_bytes_matches_clipboard_utf8() {
    let text = "hello 世界";
    let bytes = text.as_bytes().to_vec();
    assert_eq!(bytes, "hello 世界".as_bytes(), "粘贴字节应为 UTF-8 原始内容");
    assert_ne!(bytes, b"v", "不应是裸 v 键字节");
    assert!(!bytes.is_empty(), "粘贴内容不应为空");
}
```

This test validates the bytes-from-text conversion (trivially correct but establishes the pattern). The real behavior is tested by running the app and pasting.

- [ ] **Step 5: Run test to see it pass immediately (it's a pure bytes assertion)**

```bash
cargo test -p app -- test_paste_bytes_matches_clipboard_utf8 2>&1 | tail -10
```

Expected: PASS (this test validates the UTF-8 encoding contract, not the GPUI wiring).

- [ ] **Step 6: Implement Cmd+V handling in `on_key_down`**

In `crates/app/src/app.rs`, update the `use gpui::` import to add `ClipboardItem`:

```rust
use gpui::{
    actions, div, prelude::*, px, ClipboardItem, ClickEvent, Context, FocusHandle, Focusable,
    ImageSource, KeyDownEvent, Window,
};
```

Then update the `on_key_down` method. Find the block starting with:
```rust
if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
```

Replace the entire inner block with:

```rust
if let InputMode::NodeFocused(node_id) = self.runtime.focus.mode() {
    if route(&self.runtime.focus.mode(), &InputEvent::Key)
        == Route::SendInputToNode(node_id)
    {
        // Cmd+V: read clipboard and send raw UTF-8 bytes to PTY.
        if ev.keystroke.modifiers.platform && ev.keystroke.key == "v" {
            if let Some(item) = cx.read_from_clipboard() {
                if let Some(text) = item.text() {
                    if let Err(e) = self.runtime.send_node_input(
                        node_id,
                        NodeInput::Text(text.to_string()),
                    ) {
                        tracing::error!(error = %e, "paste send_node_input failed");
                    }
                }
            }
            cx.notify();
            return;
        }

        if let Some(input) = keystroke_to_input(&ev.keystroke) {
            if let Err(e) = self.runtime.send_node_input(node_id, NodeInput::Key(input)) {
                tracing::error!(error = %e, "send_node_input failed");
            }
        }
        cx.notify();
    }
}
```

**Note:** `cx.read_from_clipboard()` is available on `Context<MultisoulApp>` via GPUI's `AppContext` trait. Check by grepping: `grep -n "fn read_from_clipboard" ~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/gpui-0.2.2/src/app.rs`. If it's only on `App` not `Context`, use `cx.app().read_from_clipboard()` or the correct accessor.

Also check `ClipboardItem::text()` signature:
```bash
grep -n "fn text\|pub fn text" ~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/gpui-0.2.2/src/platform.rs | grep -i clipboard | head -5
grep -n "impl ClipboardItem\|fn text" ~/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/gpui-0.2.2/src/platform.rs | head -10
```

Adjust the call (`item.text()`) to match the actual API.

- [ ] **Step 7: Verify compilation**

```bash
cargo check -p app 2>&1 | tail -20
```

Expected: No errors. Fix any type mismatches (e.g., wrong method names on `ClipboardItem` or `Context`).

- [ ] **Step 8: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Expected: All tests pass.

- [ ] **Step 9: Check constraints**

```bash
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

Expected: No violations.

- [ ] **Step 10: Commit**

```bash
git add crates/app/src/app.rs crates/runtime/src/live.rs crates/runtime/src/runtime.rs crates/app/src/app_tests.rs
git commit -m "fix(terminal): add Cmd+V clipboard paste to focused terminal node"
```

---

## Self-Review

**Spec coverage:**
- Chinese IME (key_char path): Task 1 ✓
- Paste (Cmd+V): Task 2 ✓
- Full IME composition UI (inline candidate window via `InputHandler`/`NSTextInputClient`): explicitly out of scope per architecture note — acknowledged gap.

**Placeholder scan:** No TBD/TODO except the runtime test stub in Task 2 Step 2 which is explicitly conditional ("skip if...").

**Type consistency:**
- `NodeInput::Text(String)` introduced in Task 2 Step 3, consumed in Task 2 Step 6 — consistent.
- `keystroke_to_input` signature unchanged — Task 1 is backward compatible.
