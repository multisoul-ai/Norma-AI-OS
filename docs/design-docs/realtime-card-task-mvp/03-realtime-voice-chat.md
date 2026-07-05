# 03 实时语音聊天 SPEC

- 日期：2026-07-06
- 状态：草稿
- 上游：`01-always-listening-shell.md`、`02-realtime-sidecar-ipc.md`
- 下游：`06-task-result-callback.md`、`07-end-to-end-acceptance.md`

## 1. 目标

跑通 Always Listening 的实时语音聊天闭环。用户说话后，音频通过 Rust app 发送给 sidecar，再进入 OpenAI Realtime；assistant 的语音回复通过 sidecar 回到 Rust app 播放。这个阶段不涉及卡片任务，只验证持续对话、首响延迟和打断体验。

## 2. 范围

本阶段要做：

- 扩展 `voice_sherpa`，提供 PCM streaming capture API。
- Rust app 把 PCM chunk 发给 sidecar。
- sidecar 连接 OpenAI Realtime WebSocket。
- sidecar 把 assistant audio chunk 和 transcript/text delta 转回 Rust。
- Rust app 播放 assistant audio chunk。
- 支持用户语音打断 assistant 回复。
- 底部语音条能显示 `Listening...` / `Replying...`。

本阶段不做：

- 不处理 `enqueue_card_task`。
- 不处理 `cancel_card_task`。
- 不执行卡片命令。
- 不要求电话级低延迟。

## 3. 音频接口

建议新增：

```rust
SherpaProvider::start_pcm_stream(input_device_id)
  -> (PcmStreamSession, Receiver<PcmAudioChunk>)
```

音频 chunk：

```rust
pub struct PcmAudioChunk {
    pub sample_rate: u32,
    pub channels: u16,
    pub format: PcmFormat,
    pub bytes: Vec<u8>,
}
```

MVP 默认：

```text
format = pcm16
sample_rate = 24000
channels = 1
transport = JSONL base64
```

## 4. Realtime 流程

```text
用户打开 Always Listening
  -> app 请求麦克风权限
  -> voice_sherpa 启动 PCM stream
  -> Rust 发送 input_audio_chunk JSONL
  -> sidecar 转发给 OpenAI Realtime
  -> OpenAI Realtime 返回 assistant audio delta
  -> sidecar 发 assistant_audio_chunk JSONL
  -> Rust app 播放音频
  -> 底部语音条显示 Replying...
```

用户打断：

```text
用户在 assistant 播放时开口
  -> Rust 停止本地 assistant playback
  -> Rust 发送 interrupt
  -> sidecar 转发 Realtime interrupt / cancel response
  -> session 保持 listening
```

## 5. 技术边界

- `voice_sherpa` 只负责 macOS 麦克风权限、输入设备选择和 AVAudioEngine capture。
- `voice_sherpa` 不知道 OpenAI、sidecar、card task 或 realtime tool call。
- sidecar 只转 OpenAI Realtime WebSocket，不直接访问画布。
- app 层负责音频播放与 UI 状态。
- `runtime` 不处理音频，不依赖 realtime。

## 6. 验收点

自动验收：

- `PcmAudioChunk` metadata validation 能拒绝不支持的 format / sample_rate / channels。
- streaming API 类型满足 app background task 的 `Send` 要求。
- `input_audio_chunk` JSONL encode 包含 format、sample_rate 和 base64 data。
- `assistant_audio_chunk` JSONL decode 后能得到合法 `PcmAudioChunk`。
- interrupt command encode 包含正确 session id。

手动验收：

- 打开 Always Listening 后，对 AI 说一句普通聊天内容，AI 能用语音回复。
- AI 首响目标约为 `1-2 秒`。
- AI 回复时，用户再次开口，assistant playback 会停止或被截断，session 回到 listening。
- 本阶段即使用户说“创建卡片”，也只当普通聊天处理，不执行卡片任务。

## 7. 失败与边界

- 麦克风权限被拒绝：不启动 session，底部语音条显示错误状态。
- 输入设备不可用：不启动 PCM stream，保留 Settings 配置。
- OpenAI Realtime 断开：停止 session，底部语音条显示 `Realtime error`。
- assistant audio decode 失败：停止播放当前 chunk，记录日志；如果持续失败则进入 session error。
- JSONL base64 带来的延迟可接受；后续如首响不达标，再迁移二进制 pipe 或 Rust native provider。
