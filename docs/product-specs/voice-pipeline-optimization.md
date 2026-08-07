# 语音链路优化 SPEC

## 1. 背景与目标

**背景**：当前语音识别链路基于 Paraformer ONNX 流式识别 + Ollama 指令解析。存在两个影响使用的问题：
1. 松开快捷键时最后一批 PCM 未被完整 decode，导致 transcript 必现截断
2. transcript 存在同音字、口语化表达等误差，未经 LLM 修正直接进入指令解析

**目标**：消除截断 + 在指令解析前增加轻量 LLM transcript 修正步骤。

## 2. 范围

### 2.1 In Scope

- `crates/voice_sherpa/src/session.rs`：`InnerSession::finish()` 增加 tap 停止后 500ms 等待
- `crates/voice/src/refine.rs`：新增 `TranscriptRefiner`（Ollama 调用，专注修正不解析指令）
- `crates/app/src/app/voice/mod.rs`：在 transcript 完成后、进入指令解析前插入 refine 步骤
- `crates/storage`：`VoiceHistoryEntry` 新增 `raw_transcript` / `refined_transcript` 字段

### 2.2 Out of Scope

- 不更换 Paraformer 模型（如 offline 模式）
- 不引入 VAD 静默检测
- 不接入外部 API（Claude / GPT）
- 不改变 partial text 实时显示行为
- 不修改 `cancel()` 路径行为

## 3. 用户与使用场景

**用户**：使用语音快捷键（Cmd+Shift+V 按住）控制 canvas 操作的用户。

**典型场景**：
1. 用户按住快捷键，说"在左边新建一个 terminal 节点"，松键 → transcript 完整 → 修正同音字/术语 → 执行 canvas 指令
2. 用户快速说话到最后一刻松键 → 原来必现截断，修复后 transcript 完整
3. 语音识别把"在哪里"识别成"再哪里" → LLM 修正后传入解析层

## 4. 技术实现

### 4.1 修复一：截断问题

**根因**：`finish()` 调用 `capture.stop()` 后立即发送 stop 信号。`recognizer_loop` 收到信号后 break，此时 macOS AVAudioEngine tap 的最后一批音频可能尚未完全进入 `pcm_rx` channel，flush 段虽然存在但接收不到仍在传输中的 PCM。

**修复方案**：在 `capture.stop()` 之后 sleep 500ms，再发 `stop_tx.send(())`，确保 tap 回调完成最后一次投递。

```rust
// InnerSession::finish() 中
if let Some(capture) = self.audio.take() {
    capture.stop();
}
// Give AVAudioEngine 500ms to drain its last tap callback into pcm_tx.
std::thread::sleep(std::time::Duration::from_millis(DRAIN_WAIT_MS));
let _ = self.stop_tx.send(());
```

常量 `DRAIN_WAIT_MS: u64 = 500` 定义在 `session.rs` 顶部，便于后续调整。

**cancel() 路径不受影响**：cancel 不需要完整 transcript，保持原有立即 stop 行为。

### 4.2 修复二：LLM Transcript 修正

**新增文件**：`crates/voice/src/refine.rs`

```rust
pub struct TranscriptRefiner {
    model: String,
    endpoint: String,  // 默认 http://127.0.0.1:11434
}

pub enum RefineError {
    Timeout,
    OllamaError(String),
}

impl TranscriptRefiner {
    pub async fn refine(&self, raw: &str) -> Result<String, RefineError>;
}
```

**Prompt 任务**（系统提示）：
- 只做最小修正，不扩写，不改变意图
- 修正明显同音字错误（在/再、的/地/得等）
- 口语化专业词标准化（"ter min nal" → "terminal"）
- 英文技术术语（terminal / node / canvas / agent）保持英文，不翻译
- 禁止添加用户未说的内容
- 如无需修正，原样返回

**超时机制**：500ms tokio timeout，超时后 `RefineError::Timeout`，调用方 fallback 到原始 transcript。

**环境变量**：`MULTISOUL_REFINE_MODEL`（独立配置，默认同 `MULTISOUL_OLLAMA_MODEL` 或 `qwen2.5:1.5b`）

### 4.3 新流程（app/voice/mod.rs）

```
Paraformer transcript (raw_transcript)
    ↓
TranscriptRefiner::refine(raw, timeout=500ms)
    ↓ Ok(refined) 或 Err(timeout/error) → fallback 到 raw
    ↓
refined_transcript (用于后续解析和日志)
    ↓
OllamaVoicePlanner::plan(refined_transcript)
    ↓
execute_compiled_voice_plan
```

## 5. 数据模型变化

### VoiceHistoryEntry（storage crate）

新增两个字段：

```rust
pub raw_transcript: String,           // Paraformer 原始输出
pub refined_transcript: Option<String>, // LLM 修正后内容（None = fallback 到 raw 或未修正）
```

SQLite `voice_history` 表新增对应列，使用 `ALTER TABLE` 迁移（非破坏性）。

## 6. 非功能性要求

- refine 步骤超时 500ms 必须有 fallback，不能阻塞主链路
- cancel() 路径不增加任何延迟
- 不在 `voice_sherpa` / `voice` / `runtime` / `canvas` crate 引入 gpui 依赖
- 域 crate 职责边界不变：`voice` 负责 transcript 修正和指令规划，`voice_sherpa` 负责音频采集和 STT

## 7. 验收标准

| 场景 | 期望行为 |
|------|----------|
| 说完整句再松键 | transcript 与松键前最后一次 partial text 一致，无截断 |
| 输入语音"再哪里"（被识别为同音字错误） | refine 后输出"在哪里" |
| 输入语音说"terminal" | refined transcript 保持"terminal"，不被翻译为"终端" |
| 输入"开个新终端" | 指令解析结果与未修正时一致，意图不变 |
| refine 调用 Ollama 超时（>500ms） | fallback 到原始 transcript，继续正常解析，日志记录 refined_transcript=None |
| 取消录音（Esc） | cancel() 路径不受影响，响应时间无变化 |
| 查看 voice_history | raw_transcript 和 refined_transcript 均有记录 |

## 8. 风险与权衡

| 权衡 | 决策 |
|------|------|
| 截断修复方案：500ms delay vs VAD vs offline 模式 | 选 500ms delay，最小改动，可通过 DRAIN_WAIT_MS 调节 |
| LLM 修正模型：本地小模型 vs 外部 API | 选本地 Ollama，保持离线可用，但修正能力有限 |
| LLM 修正位置：独立步骤 vs 合并到规划 prompt | 选独立步骤，职责清晰，便于单独测试和 fallback |

**已知限制**：
- 本地小模型（qwen2.5:1.5b）对复杂同音字场景效果有限
- 500ms drain wait 会轻微增加松键到开始规划的延迟感知
