# Realtime Voice Partial Text — async_channel Event-Driven

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 录音期间在底部语音条上实时显示识别中文字，用 `async_channel` 事件驱动替代定时轮询。

**Architecture:** 把 `voice_sherpa` 的 partials channel 从 `std::sync::mpsc` 换成 `async_channel`，并将 `Receiver` 从 `SpeechSession` 中分离出来单独返回（`Receiver<String>: Send`，可以安全地 move 进 `cx.spawn` task）。App 层在录音启动后 spawn 一个 event-driven task：`partials.recv().await` 完全阻塞直到 sherpa 推来新文字，立即 `cx.notify()` 触发重绘；channel 关闭时 task 自然退出，无需手动取消。

**Tech Stack:** `async-channel = "2"`（已在 Cargo.lock 中，版本 2.5.0），GPUI `cx.spawn`，Rust `async_channel::bounded`

## Global Constraints

- `voice_sherpa` 不得引入 `gpui` 依赖（`cargo tree -p voice_sherpa | grep gpui` 应为空）
- 所有 Rust 源文件 ≤ 500 行（`bash scripts/check-max-file-lines.sh`）
- 不使用 `#[allow(...)]` 隐藏诊断
- `cargo fmt --check` 通过后才能 commit
- 单元测试注释规范遵循 `docs/quality/test-requirements.md`（场景 + 数据构造 + 执行过程 + 预期结果）

---

## File Map

| 操作 | 文件 | 改动摘要 |
|------|------|---------|
| 修改 | `crates/voice_sherpa/Cargo.toml` | 新增 `async-channel = "2"` |
| 修改 | `crates/voice_sherpa/src/session.rs` | `partial_tx/rx` 换成 `async_channel::bounded`；更新 `recognizer_loop` 参数类型 |
| 修改 | `crates/voice_sherpa/src/lib.rs` | 从 `SpeechSession` 删除 `partials` 字段；`start_recording()` 返回 `Result<(SpeechSession, async_channel::Receiver<String>), SpeechError>` |
| 修改 | `crates/app/src/app/voice/mod.rs` | 解构 `(session, partials)`；spawn event-driven partials task；删除旧的手动 drain |

---

### Task 1：`voice_sherpa` — 换 channel 类型，分离 partials receiver

**Files:**
- Modify: `crates/voice_sherpa/Cargo.toml`
- Modify: `crates/voice_sherpa/src/session.rs:70-101,147-151`
- Modify: `crates/voice_sherpa/src/lib.rs:51-55,72-75`

**Interfaces:**
- Produces: `SherpaProvider::start_recording() -> Result<(SpeechSession, async_channel::Receiver<String>), SpeechError>`
- Produces: `SpeechSession` 不再有 `partials` 字段（仅有 `inner: InnerSession`）

---

- [ ] **Step 1：在 `voice_sherpa/Cargo.toml` 中添加 `async-channel`**

  在 `[dependencies]` 区块中添加一行（无需改 workspace Cargo.toml，直接声明在 crate 级）：

  ```toml
  # crates/voice_sherpa/Cargo.toml  [dependencies] 区块末尾加：
  async-channel = "2"
  ```

- [ ] **Step 2：确认 `async-channel` 已解析成功**

  ```bash
  cargo check -p voice_sherpa 2>&1 | grep -E "error|Finished"
  ```

  预期：`Finished` 无 error（`async-channel 2.5.0` 已在 lock 中，无需网络下载）

- [ ] **Step 3：修改 `session.rs` 的 channel 声明和 `recognizer_loop` 签名**

  打开 `crates/voice_sherpa/src/session.rs`，做以下两处改动：

  **改动 A — `start()` 函数里的 channel 创建（第 80 行附近）：**

  旧：
  ```rust
  // Partials channel: recognizer thread → caller.
  let (partial_tx, partial_rx) = std::sync::mpsc::channel::<String>();
  ```

  新：
  ```rust
  // Partials channel: recognizer thread → caller.
  // bounded(64): sherpa 约 10 partials/s，64 帧足够；channel 关闭时 recv().await 返回 Err
  let (partial_tx, partial_rx) = async_channel::bounded::<String>(64);
  ```

  **改动 B — `recognizer_loop` 函数签名（第 147-152 行附近）：**

  旧：
  ```rust
  fn recognizer_loop(
      config: sherpa_onnx::OnlineRecognizerConfig,
      pcm_rx: crossbeam_channel::Receiver<Vec<f32>>,
      stop_rx: crossbeam_channel::Receiver<()>,
      partial_tx: std::sync::mpsc::Sender<String>,
  ) -> String {
  ```

  新：
  ```rust
  fn recognizer_loop(
      config: sherpa_onnx::OnlineRecognizerConfig,
      pcm_rx: crossbeam_channel::Receiver<Vec<f32>>,
      stop_rx: crossbeam_channel::Receiver<()>,
      partial_tx: async_channel::Sender<String>,
  ) -> String {
  ```

  `recognizer_loop` 内部的 `partial_tx.send(text)` 调用保持不变（`async_channel::Sender::send()` 是同步的 blocking send，在普通线程里正确可用）。

- [ ] **Step 4：修改 `lib.rs` — 从 `SpeechSession` 删除 `partials`，更新 `start_recording` 返回类型**

  打开 `crates/voice_sherpa/src/lib.rs`。

  **旧 `SpeechSession` 定义：**
  ```rust
  pub struct SpeechSession {
      /// Receive live partial transcription text (may arrive multiple times per second).
      pub partials: std::sync::mpsc::Receiver<String>,
      inner: InnerSession,
  }
  ```

  **新 `SpeechSession` 定义：**
  ```rust
  pub struct SpeechSession {
      inner: InnerSession,
  }
  ```

  **旧 `start_recording` 返回类型：**
  ```rust
  pub fn start_recording(&self) -> Result<SpeechSession, SpeechError> {
      session::start(self.model_dir.clone())
  }
  ```

  **新 `start_recording` 返回类型：**
  ```rust
  pub fn start_recording(
      &self,
  ) -> Result<(SpeechSession, async_channel::Receiver<String>), SpeechError> {
      session::start(self.model_dir.clone())
  }
  ```

- [ ] **Step 5：修改 `session.rs` 的 `start()` 返回值匹配新签名**

  `session.rs` 的 `start()` 函数末尾（第 93-101 行）目前返回 `SpeechSession { partials: partial_rx, inner: ... }`。

  旧：
  ```rust
  Ok(crate::SpeechSession {
      partials: partial_rx,
      inner: InnerSession {
          audio: Some(audio),
          state,
          thread_handle: Some(thread_handle),
          stop_tx,
      },
  })
  ```

  新（返回 tuple，`partial_rx` 单独暴露）：
  ```rust
  Ok((
      crate::SpeechSession {
          inner: InnerSession {
              audio: Some(audio),
              state,
              thread_handle: Some(thread_handle),
              stop_tx,
          },
      },
      partial_rx,
  ))
  ```

  同时更新 `start()` 函数签名本身（第 70 行附近）：

  旧：
  ```rust
  pub(crate) fn start(model_dir: PathBuf) -> Result<crate::SpeechSession, SpeechError> {
  ```

  新：
  ```rust
  pub(crate) fn start(
      model_dir: PathBuf,
  ) -> Result<(crate::SpeechSession, async_channel::Receiver<String>), SpeechError> {
  ```

- [ ] **Step 6：编译验证 `voice_sherpa` 通过**

  ```bash
  cargo check -p voice_sherpa 2>&1 | grep -E "error|warning|Finished"
  ```

  预期：`Finished`，无 `error`。若有警告检查是否为新增的（旧警告 `private_interfaces` 已存在，可忽略）。

- [ ] **Step 7：运行 `voice_sherpa` 全部测试**

  ```bash
  cargo test -p voice_sherpa 2>&1 | tail -15
  ```

  预期：`test result: ok. 10 passed; 0 failed; 1 ignored`（`test_audio_capture_sends_pcm_chunks` ignored，其余全部通过）

- [ ] **Step 8：格式化并 commit**

  ```bash
  cargo fmt -p voice_sherpa
  git add crates/voice_sherpa/
  git commit -m "feat(voice_sherpa): swap partials channel to async_channel, split receiver from SpeechSession"
  ```

---

### Task 2：`app` — event-driven partials task，删除旧手动 drain

**Files:**
- Modify: `crates/app/src/app/voice/mod.rs:48-78,140-163`

**Interfaces:**
- Consumes: `SherpaProvider::start_recording() -> Result<(SpeechSession, async_channel::Receiver<String>), SpeechError>`（Task 1 产出）

---

- [ ] **Step 1：更新 `begin_voice_recording` 里的 `start_recording` 调用**

  打开 `crates/app/src/app/voice/mod.rs`，找到 `Ok(())` 匹配臂（第 60-74 行附近）。

  旧代码（摘录关键部分）：
  ```rust
  Ok(()) => {
      match SherpaProvider::from_env_or_default()
          .and_then(|p| p.start_recording())
      {
          Ok(session) => app.voice.set_speech_session(session),
          Err(error) => {
              app.voice
                  .set_error(format!("Speech recognition failed: {error}"));
              app.save_voice_speech_error(error);
          }
      }
  }
  ```

  新代码（解构 tuple，spawn event-driven task）：
  ```rust
  Ok(()) => {
      match SherpaProvider::from_env_or_default()
          .and_then(|p| p.start_recording())
      {
          Ok((session, partials)) => {
              app.voice.set_speech_session(session);
              // Event-driven: wakes only when sherpa emits a partial,
              // exits automatically when the channel closes on session drop.
              let partials_task = cx.spawn(async move |this, cx| {
                  while let Ok(text) = partials.recv().await {
                      let _ = this.update(cx, |app, cx| {
                          app.voice.partial_text = text;
                          cx.notify();
                      });
                  }
              });
              app.voice.set_speech_task(partials_task);
          }
          Err(error) => {
              app.voice
                  .set_error(format!("Speech recognition failed: {error}"));
              app.save_voice_speech_error(error);
          }
      }
  }
  ```

  注意：
  - `partials: async_channel::Receiver<String>` 实现了 `Send`，可以安全地 `move` 进 `cx.spawn`
  - `cx.notify()` 在每次收到新文字时触发重绘；`voice_label()` → `label()` → `partial_text` 路径已存在，无需改 UI
  - 整个外层 spawn 的末尾不再调用 `set_speech_task`（partials task 已在 update 内存入）

- [ ] **Step 2：删除 `process_sherpa_speech_recording` 里的手动 drain**

  找到 `process_sherpa_speech_recording`（第 140 行附近），删除旧的手动 drain 代码。

  旧（第 148-151 行）：
  ```rust
  // Drain any partials already queued before key-up.
  while let Ok(text) = session.partials.try_recv() {
      self.voice.partial_text = text;
  }
  ```

  新：直接删除这 4 行（event-driven task 已全权处理所有 partials，包括松键前最后入队的那条）。

  `session` 的类型也不再有 `partials` 字段，所以这段代码无论如何都会 compile error，必须删除。

  删除后该函数头部应如下所示：
  ```rust
  fn process_sherpa_speech_recording(&mut self, window: &mut Window, cx: &mut Context<Self>) {
      let Some(session) = self.voice.take_speech_session() else {
          let error = SpeechError::NotRecording;
          self.voice
              .set_error(format!("Speech recognition failed: {error}"));
          self.save_voice_speech_error(error);
          return;
      };
      cx.notify();
      // finish() blocks (recognizer thread join); run in spawn_in to match existing pattern.
      let task = cx.spawn_in(window, async move |this, cx| {
  ```

- [ ] **Step 3：编译验证 app 通过**

  ```bash
  cargo check -p app 2>&1 | grep -E "error|Finished"
  ```

  预期：`Finished`，无 `error`。

- [ ] **Step 4：运行全量测试**

  ```bash
  cargo test --workspace 2>&1 | grep -E "test result|error\[" | tail -20
  ```

  预期：所有 crate 均 `test result: ok`，无 `error[E...]`。

- [ ] **Step 5：检查文件行数和约束**

  ```bash
  bash scripts/check-max-file-lines.sh
  bash scripts/check-no-allow.sh
  cargo tree -p voice_sherpa | grep -i gpui && echo "FAIL: gpui leak" || echo "ok: no gpui"
  ```

  预期：三项检查均通过（无输出 = 无超限；no gpui）。

- [ ] **Step 6：格式化并 commit**

  ```bash
  cargo fmt -p app
  git add crates/app/src/app/voice/mod.rs
  git commit -m "feat(app): event-driven partial text display via async_channel recv().await"
  ```

---

## 手动验证（需要真实麦克风）

```bash
cargo run -p app
```

1. 长按 `Cmd+Shift+V` 说话
2. 预期：底部语音条在说话过程中实时更新显示识别中的文字（无需等松键）
3. 松键后文字消失，进入 Planning / Executed 流程
4. 全程无 crash，无 `RefCell already borrowed`，无 `Input HW format` 异常
