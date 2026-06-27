# Voice Pipeline Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix voice transcript truncation on key release and add LLM-based transcript correction before command planning.

**Architecture:** (1) Add a 500ms drain wait in `PlatformSpeechSession::finish()` in `voice_macos` so AVAudioEngine can deliver the last tap callback before `endAudio()` is called. (2) New `TranscriptRefiner` in the `voice` crate makes a 500ms-timeout Ollama call to fix homophones and standardize terms, inserting between STT output and command planning. (3) Storage schema gains `raw_transcript` + `refined_transcript` columns.

**Tech Stack:** Rust, Apple Speech (SFSpeechRecognizer), objc2 bindings, Ollama HTTP API (raw TCP), rusqlite

## Global Constraints

- No `gpui` imports in `voice`, `voice_macos`, `runtime`, `canvas`, or `storage` crates
- All source files ≤ 500 lines (enforced by `scripts/check-max-file-lines.sh`)
- No `#[allow(...)]` attributes — fix root causes
- Run `cargo test -p voice -p voice_macos -p storage` after each task
- Run `cargo check -p app` after Tasks 2 and 4
- Run `bash scripts/check-max-file-lines.sh` before committing each task

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/voice_macos/src/platform.rs` | Modify | Add `DRAIN_WAIT_MS` constant and sleep in `finish()` |
| `crates/voice_macos/src/lib.rs` | Modify | Re-export `pub const DRAIN_WAIT_MS: u64 = 500` |
| `crates/voice/src/ollama.rs` | Modify | Make `post_ollama_generate` and `OllamaGenerateResponse` `pub(crate)` |
| `crates/voice/src/refine.rs` | Create | `TranscriptRefiner`, `RefineError`, `refine_prompt()` |
| `crates/voice/src/lib.rs` | Modify | Add `pub mod refine; pub use refine::{RefineError, TranscriptRefiner};` |
| `crates/voice/tests/unit/refine_tests.rs` | Create | Unit tests for prompt construction and error paths |
| `crates/storage/src/migrations.rs` | Modify | Two `ensure_column` calls for `raw_transcript`/`refined_transcript` |
| `crates/storage/src/voice_history_store.rs` | Modify | Add fields to `VoiceHistoryEntry`, `VoiceHistoryRecord`, SQL |
| `crates/storage/tests/unit/voice_history_store_tests.rs` | Modify | Update helper + add round-trip test for new fields |
| `crates/app/src/app/voice/history.rs` | **Create** | Extract `VoiceHistoryDraft` + save helpers from `mod.rs`; add `raw_transcript`/`refined_transcript` |
| `crates/app/src/app/voice/mod.rs` | Modify | Remove extracted code; add refiner call in `handle_apple_speech_result`; thread new params |

---

## Task 1: Truncation Fix (voice_macos)

**Files:**
- Modify: `crates/voice_macos/src/platform.rs`
- Modify: `crates/voice_macos/src/lib.rs`
- Modify: `crates/voice_macos/tests/unit/platform_tests.rs`

**Interfaces:**
- No interface changes — purely internal timing fix
- Produces: `DRAIN_WAIT_MS` constant exported from `voice_macos`, and `PlatformSpeechSession::finish()` that sleeps 500ms after `stop_audio()` before calling `endAudio()`

**Background:** `platform.rs` is the real macOS implementation of Apple Speech recording. `PlatformSpeechSession::finish()` calls `self.stop_audio()` (removes AVAudioEngine tap, stops engine), then immediately calls `self.request.endAudio()`. The last AVAudioPCMBuffer (~185ms of audio at 8192 frames / 44.1kHz) may not have been delivered to the `SFSpeechAudioBufferRecognitionRequest` before the engine stops, causing transcript truncation. The fix is a 500ms sleep BETWEEN `stop_audio()` and `endAudio()`. The `cancel()` path must NOT get this delay.

- [ ] **Step 1: Write a failing constant-check test**

Append to `crates/voice_macos/tests/unit/platform_tests.rs`:

```rust
/// VM-DRAIN: DRAIN_WAIT_MS 常量必须存在且 ≥ 200ms，确保 AVAudioEngine 有足够时间
/// 投递最后一个 tap 回调。
///
/// 数据构造：
///   TAP_BUFFER_SIZE = 8192 frames @ 44.1kHz ≈ 185ms per buffer
///   DRAIN_WAIT_MS 应 ≥ 200 以覆盖最长缓冲帧
///   设为 500 为保守值
///
/// 执行过程：
///   1. 读取 voice_macos::DRAIN_WAIT_MS 常量值
///   2. 断言 ≥ 200
///
/// 预期结果：
///   - 正断言：DRAIN_WAIT_MS ≥ 200
///   - 负断言：DRAIN_WAIT_MS 不应为 0（表示未添加）
#[test]
fn test_drain_wait_ms_is_adequate() {
    assert!(
        voice_macos::DRAIN_WAIT_MS >= 200,
        "DRAIN_WAIT_MS must be ≥ 200ms to cover AVAudioEngine tap buffer latency"
    );
    assert_ne!(
        voice_macos::DRAIN_WAIT_MS, 0,
        "DRAIN_WAIT_MS of 0 means no drain wait was added"
    );
}
```

- [ ] **Step 2: Run test to confirm it fails**

```bash
cargo test -p voice_macos test_drain_wait_ms_is_adequate 2>&1 | grep -E "FAILED|error\[|unresolved"
```

Expected: compile error — `DRAIN_WAIT_MS` not yet exported from `voice_macos`.

- [ ] **Step 3: Add the constant to `platform.rs`**

In `crates/voice_macos/src/platform.rs`, add after the existing constants (after `const TAP_BUFFER_SIZE: u32 = 8_192;`):

```rust
/// Milliseconds to wait after stopping the AVAudioEngine tap before calling endAudio().
/// Gives AVAudioEngine time to deliver the last tap callback into the SFSpeechAudioBufferRecognitionRequest.
pub(crate) const DRAIN_WAIT_MS: u64 = 500;
```

- [ ] **Step 4: Re-export the constant from `lib.rs`**

In `crates/voice_macos/src/lib.rs`, after the existing `pub use` blocks, add:

```rust
#[cfg(target_os = "macos")]
pub use platform::DRAIN_WAIT_MS;
#[cfg(not(target_os = "macos"))]
pub const DRAIN_WAIT_MS: u64 = 500;
```

- [ ] **Step 5: Insert the drain wait in `finish()`**

In `crates/voice_macos/src/platform.rs`, in `PlatformSpeechSession::finish()`, replace:

```rust
        self.state.apply(SessionEvent::Finish)?;
        self.stop_audio();
        unsafe {
            self.request.endAudio();
            self.task.finish();
        }
```

with:

```rust
        self.state.apply(SessionEvent::Finish)?;
        self.stop_audio();
        // Wait for AVAudioEngine to deliver the last tap callback before signalling end.
        std::thread::sleep(std::time::Duration::from_millis(DRAIN_WAIT_MS));
        unsafe {
            self.request.endAudio();
            self.task.finish();
        }
```

`cancel()` must NOT get this delay — leave it unchanged.

- [ ] **Step 6: Run the test**

```bash
cargo test -p voice_macos test_drain_wait_ms_is_adequate
```

Expected: PASS

- [ ] **Step 7: Run full voice_macos suite**

```bash
cargo test -p voice_macos
```

Expected: all pass

- [ ] **Step 8: Check file line count**

```bash
bash scripts/check-max-file-lines.sh
```

Expected: no violations

- [ ] **Step 9: Commit**

```bash
git add crates/voice_macos/src/platform.rs crates/voice_macos/src/lib.rs crates/voice_macos/tests/unit/platform_tests.rs
git commit -m "fix(voice_macos): add 500ms drain wait in finish() to prevent transcript truncation"
```

---

## Task 2: Storage Schema — raw_transcript and refined_transcript

**Files:**
- Modify: `crates/storage/src/migrations.rs`
- Modify: `crates/storage/src/voice_history_store.rs`
- Modify: `crates/storage/tests/unit/voice_history_store_tests.rs`

**Interfaces:**
- Consumes: existing `VoiceHistoryEntry`, `VoiceHistoryRecord`, `ensure_column`
- Produces:
  - `VoiceHistoryEntry { raw_transcript: String, refined_transcript: Option<String>, ... }`
  - `VoiceHistoryRecord { raw_transcript: String, refined_transcript: Option<String>, ... }`
  - Updated `save_entry` / `load_recent` SQL (new columns at positions 11, 12)

- [ ] **Step 1: Write failing tests**

Replace the existing `history_entry` helper and add one new test at the bottom of `crates/storage/tests/unit/voice_history_store_tests.rs`.

Update the `history_entry` helper to include the new fields (this makes existing tests compile-fail, confirming TDD):

```rust
fn history_entry(transcript: &str, status: &str) -> VoiceHistoryEntry {
    VoiceHistoryEntry {
        transcript: transcript.to_string(),
        raw_transcript: transcript.to_string(),
        refined_transcript: None,
        planner_json: r#"{"commands":[]}"#.to_string(),
        confidence: 0.82,
        risk: "low".to_string(),
        status: status.to_string(),
        provider: Some("ollama".to_string()),
        model: Some("llama3.1".to_string()),
        target_node_id: Some(7),
        error: None,
    }
}
```

Add new test at the end of the file:

```rust
/// VH-3: raw_transcript 和 refined_transcript 应分别保存并可读回。
///
/// 数据构造（含关键数值的推导过程）：
///   raw_transcript = "再哪里" （Apple Speech 原始输出）
///   refined_transcript = Some("在哪里") （LLM 修正后）
///   transcript = "在哪里" （用于规划）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 entry，raw 与 refined 不同
///   2. save_entry(entry)
///   3. load_recent(1)
///   4. 检查两列独立存储
///
/// 预期结果：
///   - 正断言：raw_transcript = "再哪里"
///   - 正断言：refined_transcript = Some("在哪里")
///   - 负断言：raw_transcript 不应等于 refined_transcript（它们不同）
#[test]
fn test_voice_history_stores_raw_and_refined_transcripts() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = VoiceHistoryStore::new(temp.path().join("project.sqlite"));
    let entry = VoiceHistoryEntry {
        transcript: "在哪里".to_string(),
        raw_transcript: "再哪里".to_string(),
        refined_transcript: Some("在哪里".to_string()),
        planner_json: r#"{"commands":[]}"#.to_string(),
        confidence: 0.9,
        risk: "low".to_string(),
        status: "executed".to_string(),
        provider: Some("ollama".to_string()),
        model: Some("qwen2.5:1.5b".to_string()),
        target_node_id: None,
        error: None,
    };

    store.save_entry(&entry).expect("entry 应写入 SQLite");
    let records = store.load_recent(1).expect("应可从 SQLite 读回");

    assert_eq!(records.len(), 1, "应读回 1 条记录");
    assert_eq!(
        records[0].raw_transcript, "再哪里",
        "raw_transcript 应保存 Apple Speech 原始输出"
    );
    assert_eq!(
        records[0].refined_transcript,
        Some("在哪里".to_string()),
        "refined_transcript 应保存 LLM 修正后内容"
    );
    assert_ne!(
        records[0].raw_transcript, records[0].refined_transcript.as_deref().unwrap_or(""),
        "同音字修正场景下 raw 与 refined 不应相同"
    );
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p storage 2>&1 | grep -E "FAILED|error\[E"
```

Expected: compile errors — `raw_transcript` / `refined_transcript` fields missing.

- [ ] **Step 3: Add migration columns**

In `crates/storage/src/migrations.rs`, after the existing `ensure_column` calls in `migrate_project_db`, add:

```rust
    ensure_column(
        connection,
        "voice_history",
        "raw_transcript",
        "raw_transcript TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(
        connection,
        "voice_history",
        "refined_transcript",
        "refined_transcript TEXT",
    )?;
```

- [ ] **Step 4: Update `VoiceHistoryEntry` and `VoiceHistoryRecord`**

In `crates/storage/src/voice_history_store.rs`, add to both structs:

```rust
pub struct VoiceHistoryEntry {
    pub transcript: String,
    pub raw_transcript: String,              // NEW — Apple Speech raw output
    pub refined_transcript: Option<String>,  // NEW — LLM-corrected, None = fallback or unchanged
    pub planner_json: String,
    pub confidence: f64,
    pub risk: String,
    pub status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub target_node_id: Option<NodeId>,
    pub error: Option<String>,
}

pub struct VoiceHistoryRecord {
    pub id: i64,
    pub transcript: String,
    pub raw_transcript: String,
    pub refined_transcript: Option<String>,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: String,
    pub status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub target_node_id: Option<NodeId>,
    pub created_at_unix: u64,
    pub error: Option<String>,
}
```

- [ ] **Step 5: Update `save_entry` SQL**

Replace the `save_entry` implementation:

```rust
    pub fn save_entry(&self, entry: &VoiceHistoryEntry) -> StorageResult<i64> {
        let connection = open_project_connection(&self.path)?;
        connection.execute(
            r#"
            INSERT INTO voice_history (
                transcript, planner_json, confidence, risk, status, provider, model,
                target_node_id, created_at_unix, error, raw_transcript, refined_transcript
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                entry.transcript,
                entry.planner_json,
                entry.confidence,
                entry.risk,
                entry.status,
                entry.provider,
                entry.model,
                entry.target_node_id,
                current_unix_timestamp()?,
                entry.error,
                entry.raw_transcript,
                entry.refined_transcript,
            ],
        )?;
        Ok(connection.last_insert_rowid())
    }
```

- [ ] **Step 6: Update `load_recent` SQL**

Replace the `load_recent` implementation:

```rust
    pub fn load_recent(&self, limit: usize) -> StorageResult<Vec<VoiceHistoryRecord>> {
        let connection = open_project_connection(&self.path)?;
        let mut statement = connection.prepare(
            r#"
            SELECT id, transcript, planner_json, confidence, risk, status, provider, model,
                   target_node_id, created_at_unix, error, raw_transcript, refined_transcript
            FROM voice_history
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok(VoiceHistoryRecord {
                id: row.get(0)?,
                transcript: row.get(1)?,
                planner_json: row.get(2)?,
                confidence: row.get(3)?,
                risk: row.get(4)?,
                status: row.get(5)?,
                provider: row.get(6)?,
                model: row.get(7)?,
                target_node_id: row.get(8)?,
                created_at_unix: row.get(9)?,
                error: row.get(10)?,
                raw_transcript: row.get(11)?,
                refined_transcript: row.get(12)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
```

- [ ] **Step 7: Run storage tests**

```bash
cargo test -p storage
```

Expected: all pass including `test_voice_history_stores_raw_and_refined_transcripts`

- [ ] **Step 8: Check file line count**

```bash
bash scripts/check-max-file-lines.sh
```

Expected: no violations

- [ ] **Step 9: Commit**

```bash
git add crates/storage/src/migrations.rs crates/storage/src/voice_history_store.rs crates/storage/tests/unit/voice_history_store_tests.rs
git commit -m "feat(storage): add raw_transcript and refined_transcript columns to voice_history"
```

---

## Task 3: TranscriptRefiner in voice crate

**Files:**
- Modify: `crates/voice/src/ollama.rs` (expose internal HTTP helpers as `pub(crate)`)
- Create: `crates/voice/src/refine.rs`
- Create: `crates/voice/tests/unit/refine_tests.rs`
- Modify: `crates/voice/src/lib.rs`

**Interfaces:**
- Consumes: `pub(crate) post_ollama_generate`, `pub(crate) OllamaGenerateResponse` from `ollama.rs`; `VoicePlanProviderError` (already `pub(crate)` accessible inside crate)
- Produces:
  - `pub struct TranscriptRefiner` with `fn new(endpoint, model) -> Self`, `fn from_env_or_default() -> Self`, `fn model(&self) -> &str`, `fn refine(&self, raw: &str) -> Result<String, RefineError>`
  - `pub enum RefineError { Timeout, OllamaError(String) }`
  - `pub fn refine_prompt(raw: &str) -> String` (exported for tests)

- [ ] **Step 1: Write failing tests**

Create `crates/voice/tests/unit/refine_tests.rs`:

```rust
use super::{refine_prompt, RefineError, TranscriptRefiner};

/// R-1: from_env_or_default 应读取 MULTISOUL_REFINE_MODEL 优先于 MULTISOUL_OLLAMA_MODEL。
///
/// 数据构造：
///   MULTISOUL_REFINE_MODEL = "custom-refine-model"
///   MULTISOUL_OLLAMA_MODEL = "base-model"（应被忽略）
///
/// 执行过程：
///   1. 设置两个 env var
///   2. 调用 from_env_or_default
///   3. 检查 model 字段
///
/// 预期结果：
///   - 正断言：model = "custom-refine-model"
///   - 负断言：model 不应为 "base-model"
#[test]
fn test_refiner_uses_refine_model_env_var_over_ollama_model() {
    // Safety: test-only env mutation, single-threaded test runner.
    unsafe {
        std::env::set_var("MULTISOUL_REFINE_MODEL", "custom-refine-model");
        std::env::set_var("MULTISOUL_OLLAMA_MODEL", "base-model");
    }
    let refiner = TranscriptRefiner::from_env_or_default();
    assert_eq!(
        refiner.model(), "custom-refine-model",
        "MULTISOUL_REFINE_MODEL should take priority when set"
    );
    assert_ne!(
        refiner.model(), "base-model",
        "MULTISOUL_OLLAMA_MODEL must not override MULTISOUL_REFINE_MODEL"
    );
    unsafe {
        std::env::remove_var("MULTISOUL_REFINE_MODEL");
        std::env::remove_var("MULTISOUL_OLLAMA_MODEL");
    }
}

/// R-2: refine() 在连接拒绝时应返回 RefineError::Timeout（不 panic，不阻塞）。
///
/// 数据构造：
///   endpoint = http://127.0.0.1:19999 （无监听端口）
///   timeout = 500ms
///
/// 执行过程：
///   1. 构造指向无效端口的 refiner
///   2. 调用 refine("test input")
///   3. 检查错误类型
///
/// 预期结果：
///   - 正断言：返回 Err(RefineError::Timeout)
///   - 负断言：不应 panic 或挂起超过 1 秒
#[test]
fn test_refiner_returns_timeout_on_connection_refused() {
    let refiner = TranscriptRefiner::new(
        "http://127.0.0.1:19999".to_string(),
        "qwen2.5:1.5b".to_string(),
    );
    let result = refiner.refine("test input");
    assert!(
        matches!(result, Err(RefineError::Timeout)),
        "connection refused should map to RefineError::Timeout, got {:?}", result
    );
}

/// R-3: refine_prompt 应包含原始 transcript 并禁止扩写。
///
/// 数据构造：
///   raw = "再哪里"
///
/// 执行过程：
///   1. 调用 refine_prompt("再哪里")
///   2. 检查 prompt 字符串
///
/// 预期结果：
///   - 正断言：prompt 包含 "再哪里"
///   - 正断言：prompt 包含 "English" 或 "英文" 相关指令
///   - 负断言：prompt 不应为空
#[test]
fn test_refine_prompt_contains_transcript_and_english_constraint() {
    let prompt = refine_prompt("再哪里");
    assert!(
        prompt.contains("再哪里"),
        "prompt must embed the raw transcript verbatim"
    );
    assert!(
        !prompt.is_empty(),
        "prompt must not be empty"
    );
    assert!(
        prompt.to_ascii_lowercase().contains("english") || prompt.contains("英文"),
        "prompt must instruct the model to keep English technical terms"
    );
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```bash
cargo test -p voice refine 2>&1 | grep -E "FAILED|error\[E"
```

Expected: compile errors — `TranscriptRefiner`, `RefineError` not found.

- [ ] **Step 3: Expose HTTP helpers from `ollama.rs`**

In `crates/voice/src/ollama.rs`, change:

```rust
#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}
```

to:

```rust
#[derive(Deserialize)]
pub(crate) struct OllamaGenerateResponse {
    pub(crate) response: String,
}
```

And change:

```rust
fn post_ollama_generate(
```

to:

```rust
pub(crate) fn post_ollama_generate(
```

- [ ] **Step 4: Create `refine.rs`**

Create `crates/voice/src/refine.rs`:

```rust
use crate::ollama::{post_ollama_generate, VoicePlanProviderError};
use std::time::Duration;

const REFINE_TIMEOUT_MS: u64 = 500;

pub struct TranscriptRefiner {
    endpoint: String,
    pub(crate) model: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RefineError {
    #[error("transcript refinement timed out")]
    Timeout,
    #[error("ollama error: {0}")]
    OllamaError(String),
}

impl TranscriptRefiner {
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
        }
    }

    pub fn from_env_or_default() -> Self {
        let model = std::env::var("MULTISOUL_REFINE_MODEL")
            .or_else(|_| std::env::var("MULTISOUL_OLLAMA_MODEL"))
            .unwrap_or_else(|_| "qwen2.5:1.5b".to_string());
        let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
            .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
        Self::new(endpoint, model)
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Correct `raw` transcript with minimal changes. Returns the corrected string,
    /// or `RefineError::Timeout` if Ollama does not respond within 500ms.
    pub fn refine(&self, raw: &str) -> Result<String, RefineError> {
        let prompt = refine_prompt(raw);
        let response =
            post_ollama_generate(&self.endpoint, &self.model, &prompt, Duration::from_millis(REFINE_TIMEOUT_MS))
                .map_err(|e| match e {
                    VoicePlanProviderError::Io(_) => RefineError::Timeout,
                    other => RefineError::OllamaError(other.to_string()),
                })?;
        // Expected response: {"refined": "<corrected text>"}
        let val: serde_json::Value = serde_json::from_str(&response.response)
            .map_err(|e| RefineError::OllamaError(e.to_string()))?;
        val["refined"]
            .as_str()
            .map(|s| s.trim().to_string())
            .ok_or_else(|| RefineError::OllamaError("missing 'refined' field in response".to_string()))
    }
}

pub fn refine_prompt(raw: &str) -> String {
    format!(
        "You are a transcript correction assistant for a voice-controlled macOS app.\n\n\
         TASK: Correct the transcript below with minimal changes.\n\
         - Fix obvious homophone errors (在/再, 的/地/得, etc.)\n\
         - Standardize technical terms spoken phonetically (\"ter min nal\" → \"terminal\")\n\
         - Keep English technical terms in English (terminal, node, canvas, agent, codex)\n\
         - Do NOT add content the user did not say\n\
         - Do NOT rephrase or expand sentences\n\
         - If no correction is needed, return the transcript unchanged\n\n\
         Return exactly one JSON object: {{\"refined\": \"<corrected transcript>\"}}\n\n\
         TRANSCRIPT: {}",
        raw
    )
}

#[cfg(test)]
#[path = "../tests/unit/refine_tests.rs"]
mod tests;
```

- [ ] **Step 5: Update `lib.rs`**

In `crates/voice/src/lib.rs`, add:

```rust
mod ollama;
mod ollama_repair;
mod plan;
pub mod refine;

pub use ollama::{OllamaVoicePlanner, VoicePlanner};
pub use plan::{
    compile_voice_plan, CardRef, CompiledVoicePlan, ExecutableCommand, PlannedCardKind,
    PlannedCommand, PlannedTarget, RiskLevel, Transcript, VoiceCardKind, VoicePlan,
    VoicePlanDecision, VoicePlanError, VoicePlanningRequest, MIN_EXECUTE_CONFIDENCE,
};
pub use refine::{refine_prompt, RefineError, TranscriptRefiner};
```

- [ ] **Step 6: Run voice tests**

```bash
cargo test -p voice
```

Expected: all pass including R-1, R-2, R-3

- [ ] **Step 7: Check file line count**

```bash
bash scripts/check-max-file-lines.sh
```

Expected: no violations

- [ ] **Step 8: Commit**

```bash
git add crates/voice/src/refine.rs crates/voice/src/lib.rs crates/voice/src/ollama.rs crates/voice/tests/unit/refine_tests.rs
git commit -m "feat(voice): add TranscriptRefiner for LLM-based transcript correction"
```

---

## Task 4: App Wiring

**Files:**
- **Create**: `crates/app/src/app/voice/history.rs`
- Modify: `crates/app/src/app/voice/mod.rs`

**Interfaces:**
- Consumes: `TranscriptRefiner::from_env_or_default()`, `TranscriptRefiner::refine()` from Task 3; updated `VoiceHistoryEntry` from Task 2
- Produces: history helpers in own file; refiner call in `handle_apple_speech_result`; both raw + refined threaded to all `save_voice_history` call sites

**Background on `mod.rs` current state:** `mod.rs` is 491 lines and contains:
- `VoiceHistoryDraft` struct (lines ~27-36) — to be extracted
- `save_voice_history`, `save_voice_speech_error`, `save_voice_history_with_provider` impl methods (lines ~398-447) — to be extracted
- `handle_apple_speech_result` function — correct function name (NOT `handle_sherpa_speech_result`)
- `process_apple_speech_recording` function — correct function name
- Import: `use voice_macos::{AppleSpeechProvider, SpeechError};`
- `handle_voice_transcript` function — where planning happens

Adding the refiner call and updated `save_voice_history` signature would push `mod.rs` over 500 lines. Extracting ~40 lines to `history.rs` keeps both files under the limit.

- [ ] **Step 1: Write a failing compile test**

Add to `crates/app/tests/unit/app_voice_tests.rs` (at the end):

```rust
/// AV-RAW: history helper は raw_transcript を受け取れる新シグネチャを持つべき。
/// このテストは save_voice_history の新シグネチャがコンパイルされることを保証する。
///
/// 実行過程：
///   1. voice_transcript_source_from_value に Some("test") を渡す
///   2. Environment variant が返ることを確認
///
/// 期待結果：
///   - 正断言：Environment variant が返る
///   - 負断言：AppleSpeech が返らない
#[test]
fn test_voice_transcript_source_env_returns_environment_variant() {
    let source = voice_transcript_source_from_value(Some("raw test".to_string()));
    assert!(
        matches!(source, VoiceTranscriptSource::Environment(ref s) if s == "raw test"),
        "Some env value should produce Environment variant with exact transcript"
    );
    let apple_source = voice_transcript_source_from_value(None);
    assert!(
        matches!(apple_source, VoiceTranscriptSource::AppleSpeech),
        "None should produce AppleSpeech variant"
    );
}
```

Run to confirm it passes (this test is compile-only for the voice source helpers, which aren't changing in this task):

```bash
cargo test -p app test_voice_transcript_source_env_returns_environment_variant 2>&1 | grep -E "FAILED|error\[|ok"
```

- [ ] **Step 2: Create `history.rs` by extracting from `mod.rs`**

Create `crates/app/src/app/voice/history.rs` with the extracted code plus new fields:

```rust
//! Voice history persistence helpers.

use crate::app::MultisoulApp;
use storage::VoiceHistoryEntry;
use voice::OllamaVoicePlanner;
use voice_macos::{AppleSpeechProvider, SpeechError};

pub(super) struct VoiceHistoryDraft {
    pub raw_transcript: String,
    pub refined_transcript: Option<String>,
    pub planner_json: String,
    pub confidence: f64,
    pub risk: &'static str,
    pub status: &'static str,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
}

impl MultisoulApp {
    pub(crate) fn save_voice_history(
        &mut self,
        raw_transcript: String,
        refined_transcript: Option<String>,
        planner_json: String,
        confidence: f64,
        risk: &'static str,
        status: &'static str,
        error: Option<String>,
    ) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            raw_transcript,
            refined_transcript,
            planner_json,
            confidence,
            risk,
            status,
            provider: Some("ollama".to_string()),
            model: Some(OllamaVoicePlanner::configured_model()),
            error,
        });
    }

    pub(super) fn save_voice_speech_error(&mut self, error: SpeechError) {
        self.save_voice_history_with_provider(VoiceHistoryDraft {
            raw_transcript: "".into(),
            refined_transcript: None,
            planner_json: "{}".into(),
            confidence: 0.0,
            risk: "low",
            status: "provider_error",
            provider: Some("apple_speech".to_string()),
            model: Some(AppleSpeechProvider::default_locale()),
            error: Some(error.to_string()),
        });
    }

    pub(super) fn save_voice_history_with_provider(&mut self, draft: VoiceHistoryDraft) {
        // transcript used for planning = refined if available, else raw
        let transcript = draft
            .refined_transcript
            .clone()
            .unwrap_or_else(|| draft.raw_transcript.clone());
        let entry = VoiceHistoryEntry {
            transcript,
            raw_transcript: draft.raw_transcript,
            refined_transcript: draft.refined_transcript,
            planner_json: draft.planner_json,
            confidence: draft.confidence,
            risk: draft.risk.to_string(),
            status: draft.status.to_string(),
            provider: draft.provider,
            model: draft.model,
            target_node_id: None,
            error: draft.error,
        };
        if let Err(error) = self.persistence.save_voice_history(&entry) {
            tracing::warn!(%error, "voice history save failed");
        }
    }
}
```

- [ ] **Step 3: Update `mod.rs`**

In `crates/app/src/app/voice/mod.rs`:

**3a. Add `pub(crate) mod history;`** to the module declarations at the top:

```rust
pub(crate) mod control;
pub(crate) mod history;
pub(crate) mod log;
pub(crate) mod modal;
```

**3b. Remove the old `VoiceHistoryDraft` struct** (lines ~27-36 — the struct definition including all fields through the closing brace).

**3c. Remove the old impl block methods** `save_voice_history`, `save_voice_speech_error`, `save_voice_history_with_provider` from `mod.rs` (lines ~398-447).

**3d. Add import for `history::VoiceHistoryDraft`:**

```rust
use crate::app::voice::history::VoiceHistoryDraft;
```

**3e. Add `TranscriptRefiner` to the `voice` import block:**

```rust
use voice::{
    compile_voice_plan, CardRef, ExecutableCommand, OllamaVoicePlanner, RiskLevel, TranscriptRefiner,
    VoiceCardKind, VoicePlanDecision, VoicePlanner, VoicePlanningRequest,
};
```

**3f. Replace `handle_apple_speech_result` body** to add refiner call:

```rust
    fn handle_apple_speech_result(
        &mut self,
        result: Result<String, SpeechError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(raw) if !raw.trim().is_empty() => {
                let raw = raw.trim().to_string();
                let refiner = TranscriptRefiner::from_env_or_default();
                let (planning_transcript, refined_transcript) = match refiner.refine(&raw) {
                    Ok(refined) if refined != raw => (refined.clone(), Some(refined)),
                    _ => (raw.clone(), None),
                };
                self.handle_voice_transcript(
                    planning_transcript,
                    raw,
                    refined_transcript,
                    window,
                    cx,
                );
            }
            Ok(_) | Err(SpeechError::EmptyTranscript) => {
                self.record_empty_voice_transcript();
            }
            Err(error) => {
                self.voice
                    .set_error(format!("Speech recognition failed: {error}"));
                self.save_voice_speech_error(error);
            }
        }
    }
```

**3g. Update the `Environment` branch in `process_finished_voice_recording`:**

```rust
            VoiceTranscriptSource::Environment(transcript) if !transcript.trim().is_empty() => {
                let t = transcript.trim().to_string();
                self.handle_voice_transcript(t.clone(), t, None, window, cx);
            }
            VoiceTranscriptSource::Environment(_) => {
                self.record_empty_voice_transcript();
            }
```

**3h. Update `handle_voice_transcript` signature** to take `raw_transcript` and `refined_transcript`:

```rust
    fn handle_voice_transcript(
        &mut self,
        transcript: String,
        raw_transcript: String,
        refined_transcript: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
```

**3i. Update all six `save_voice_history` call sites** inside `handle_voice_transcript` to pass `raw_transcript.clone(), refined_transcript.clone()` as the first two arguments:

1. Planner failed:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    "{}".into(),
                    0.0,
                    "low",
                    "provider_error",
                    Some(error.to_string()),
                );
```

2. Planner rejected:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    planner_json,
                    confidence,
                    risk,
                    "planner_rejected",
                    Some(error.to_string()),
                );
```

3. TranscriptOnly:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    planner_json,
                    confidence,
                    risk,
                    "transcript_only",
                    None,
                );
```

4. Confirmation required:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    planner_json,
                    confidence,
                    risk,
                    "confirmation_required",
                    None,
                );
```

5. Executed:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    planner_json,
                    confidence,
                    risk,
                    "executed",
                    None,
                );
```

6. Runtime error:
```rust
                self.save_voice_history(
                    raw_transcript.clone(),
                    refined_transcript.clone(),
                    planner_json,
                    confidence,
                    risk,
                    "runtime_error",
                    Some(error),
                );
```

**3j. Update `record_empty_voice_transcript`:**

```rust
    fn record_empty_voice_transcript(&mut self) {
        self.voice.set_transcript_only("", "empty transcript");
        self.save_voice_history("".into(), None, "{}".into(), 0.0, "low", "transcript_only", None);
    }
```

- [ ] **Step 4: Run `cargo check -p app`**

```bash
cargo check -p app 2>&1 | grep -E "error\[|^error"
```

Expected: no errors

- [ ] **Step 5: Run workspace tests**

```bash
cargo test --workspace
```

Expected: all pass

- [ ] **Step 6: Check file line count**

```bash
bash scripts/check-max-file-lines.sh
```

Expected: no violations

- [ ] **Step 7: Commit**

```bash
git add crates/app/src/app/voice/history.rs crates/app/src/app/voice/mod.rs
git commit -m "feat(app): wire TranscriptRefiner into voice pipeline; thread raw/refined through history"
```

---

## Post-Plan Checklist

- [ ] All four tasks committed on branch `worktree-voice-pipeline-optimization`
- [ ] `cargo test --workspace` passes
- [ ] `bash scripts/check-max-file-lines.sh` passes
- [ ] `bash scripts/check-no-allow.sh` passes
- [ ] `cargo check -p app` passes
