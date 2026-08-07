# Voice Input Device Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Settings → Voice 中支持选择语音输入麦克风，让耳机麦克风可作为语音识别输入源。

**Architecture:** `voice_sherpa` 暴露零 GPUI 的音频输入设备模型、枚举和录音启动参数；`app` 的 Voice Settings 负责持久化选中的设备 ID 并渲染内联设备列表；录音开始时把选中的设备 ID 传入 `SherpaProvider`。显式设备录音在 macOS 上改走 `AVCaptureDevice + AVCaptureSession + AVCaptureAudioDataOutput`，避免 `AVAudioEngine::inputNode()` 只能跟随系统默认输入的问题。

**Tech Stack:** Rust 2021, GPUI 0.2.2, objc2-av-foundation 0.3.2, objc2-avf-audio 0.3.2, sherpa-onnx 1.13, crossbeam-channel.

## Global Constraints

- 写 production code 前必须先写会失败的聚焦测试，并确认失败原因正确。
- 新增测试必须有完整中文测试注释：场景、数据构造、执行过程、预期结果。
- 每个 `assert!` / `assert_eq!` 必须带说明字符串；关键状态要有正断言和负断言。
- `voice_sherpa` 保持零 GPUI；`app` 可以依赖 `voice_sherpa`，反向不允许。
- 选中的麦克风设备缺失时必须报错提示，不能静默回退到 System Default。
- 切换输入源只影响下一次录音，不热切当前录音。
- v1 不实现输出源配置。

---

### Task 1: voice_sherpa 设备选择纯逻辑

**Files:**
- Create: `crates/voice_sherpa/src/devices.rs`
- Modify: `crates/voice_sherpa/src/lib.rs`
- Test: `crates/voice_sherpa/tests/unit/device_tests.rs`

**Interfaces:**
- Produces:
  - `pub struct AudioInputDevice { pub id: String, pub name: String, pub is_default: bool }`
  - `pub fn resolve_audio_input_device_selection(selected_id: Option<&str>, devices: &[AudioInputDevice]) -> Result<Option<AudioInputDevice>, SpeechError>`
  - `pub fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, SpeechError>`

- [ ] **Step 1: Write failing tests**

Add tests covering:
- `None` returns the default device when present.
- Existing selected ID returns that exact device.
- Missing selected ID returns `SpeechError::Audio` containing the selected ID.
- Empty/blank selected ID is treated as System Default.

- [ ] **Step 2: Run RED**

Run: `cargo test -p voice_sherpa device`

Expected: FAIL because `devices` module and resolver do not exist.

- [ ] **Step 3: Implement pure model and resolver**

Create `devices.rs` with the public struct and resolver. Add a macOS `list_audio_input_devices` implementation using `AVCaptureDevice::devicesWithMediaType(AVMediaTypeAudio)` and `defaultDeviceWithMediaType`; non-macOS returns `SpeechError::Audio("audio input devices not supported on this platform")`.

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p voice_sherpa device`

Expected: PASS.

### Task 2: Settings state persistence for microphone selection

**Files:**
- Modify: `crates/app/src/settings/voice/state.rs`
- Test: `crates/app/tests/unit/voice_settings_tests.rs`

**Interfaces:**
- Consumes: `voice_sherpa::AudioInputDevice`
- Produces:
  - saved key `voice.input_device_id`
  - env key `MULTISOUL_VOICE_INPUT_DEVICE_ID`
  - `VoiceSettingsValues::input_device_id(&self) -> Option<&str>`
  - `VoiceSettingsState::select_input_device(&mut self, id: Option<String>)`
  - `VoiceSettingsState::refresh_input_devices(&mut self)`

- [ ] **Step 1: Write failing tests**

Add tests covering:
- saved `input_device_id` overrides env.
- env `input_device_id` is used when saved is missing.
- `select_input_device(Some(...))` persists and resolves the value.
- `select_input_device(None)` clears the saved value and resolves to System Default.
- `refresh_input_devices` failure preserves existing selected ID and stores an error status.

- [ ] **Step 2: Run RED**

Run: `cargo test -p app voice_settings`

Expected: FAIL because input device fields/methods do not exist.

- [ ] **Step 3: Implement state changes**

Extend `SavedVoiceSettings`, `EnvVoiceSettings`, and `VoiceSettingsValues` with `input_device_id`. Load from `SettingsStore`, env, and resolver using the same saved-over-env precedence as existing voice settings. Store device list and refresh error in `VoiceSettingsState`.

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p app voice_settings`

Expected: PASS.

### Task 3: Recording API passes selected input device

**Files:**
- Modify: `crates/voice_sherpa/src/lib.rs`
- Modify: `crates/voice_sherpa/src/session.rs`
- Modify: `crates/voice_sherpa/src/audio.rs`
- Modify: `crates/app/src/app/voice/mod.rs`
- Test: `crates/app/tests/unit/app_voice_tests.rs`

**Interfaces:**
- Consumes: `VoiceSettingsValues::input_device_id`
- Produces:
  - `SherpaProvider::start_recording_with_input_device(input_device_id: Option<&str>)`
  - `session::start(model_dir: PathBuf, input_device_id: Option<String>)`
  - `AudioCapture::start(sender, stats, input_device_id)`

- [ ] **Step 1: Write failing tests**

Add focused app test proving voice recording passes the selected input device ID into a test seam, and no ID is passed when System Default is selected.

- [ ] **Step 2: Run RED**

Run: `cargo test -p app app_voice`

Expected: FAIL because recording API does not accept an input device ID.

- [ ] **Step 3: Implement API threading**

Add the new provider/session/audio parameters while keeping existing `start_recording()` as a convenience wrapper calling `start_recording_with_input_device(None)`. Update `begin_voice_recording` to pass `app.voice_settings.values().input_device_id()`.

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p app app_voice`

Expected: PASS.

### Task 4: macOS explicit capture implementation

**Files:**
- Modify: `crates/voice_sherpa/Cargo.toml`
- Modify: `crates/voice_sherpa/src/audio.rs`
- Test: `crates/voice_sherpa/tests/unit/audio_tests.rs`

**Interfaces:**
- Consumes: selected input device ID from Task 3.
- Produces: macOS `AudioCapture` that uses System Default when ID is `None`, and selected `AVCaptureDevice` when ID is `Some`.

- [ ] **Step 1: Write failing pure conversion tests**

Add pure tests for sample conversion helpers:
- mono `f32` samples pass through.
- stereo interleaved/planar samples collapse to channel 0.
- zero sample rate returns `SpeechError::Audio`.

- [ ] **Step 2: Run RED**

Run: `cargo test -p voice_sherpa audio`

Expected: FAIL because conversion helpers do not exist.

- [ ] **Step 3: Implement capture**

Add required objc2 features for `AVCaptureSession`, `AVCaptureDeviceInput`, `AVCaptureAudioDataOutput`, `AVCaptureOutput`, `AVCaptureOutputBase`, `objc2-core-media`, and `dispatch2`. Implement `AVCaptureSession` ownership, delegate callback, nonblocking `try_send`, resampling to 16 kHz, and `stopRunning` cleanup. If selected device is unavailable, return `SpeechError::Audio("selected microphone is unavailable: <id>")`.

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p voice_sherpa audio`

Expected: PASS.

### Task 5: Settings UI inline device list

**Files:**
- Modify: `crates/app/src/settings/voice/view.rs`
- Modify: `crates/app/src/settings/voice.rs`
- Test: `crates/app/tests/unit/settings_panel_tests.rs`

**Interfaces:**
- Consumes: `VoiceSettingsState::input_devices`, `select_input_device`, and `refresh_input_devices`.
- Produces: visible Settings rows for System Default, each microphone, Refresh, and Use system default.

- [ ] **Step 1: Write failing UI anchor tests**

Extend settings panel tests to assert anchors exist:
- `settings-voice-input-device-default`
- `settings-voice-input-device-refresh`
- `settings-voice-input-device-clear`

- [ ] **Step 2: Run RED**

Run: `cargo test -p app settings_panel`

Expected: FAIL because UI anchors do not exist.

- [ ] **Step 3: Implement UI**

Render an inline “Microphone Input” group under Speech Recognition. Keep the current row/button style, mark selected row with accent border/text, and show refresh errors through existing status message styling.

- [ ] **Step 4: Run GREEN**

Run: `cargo test -p app settings_panel`

Expected: PASS.

### Task 6: Final verification

**Files:**
- All changed files.

- [ ] **Step 1: Run focused test suite**

Run:
```bash
cargo test -p voice_sherpa
cargo test -p app voice_settings
cargo test -p app settings_panel
cargo test -p app app_voice
cargo check -p app
bash scripts/check-no-allow.sh
```

Expected: all pass.

- [ ] **Step 2: Review status**

Run: `git status --short`

Expected: only plan, voice_sherpa, and app voice settings/voice flow files changed.
