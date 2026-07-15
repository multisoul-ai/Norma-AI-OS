# 实时对话 TTS 市场架构研究

> 日期：2026-07-11
> 范围：实时语音 Agent 中的文本切分、增量 TTS、播放、打断，以及本地 macOS 中文落地。
> 方法：仅使用厂商官方文档、官方仓库和论文；厂商自报延迟只作为候选方案线索，不视为 Norma OS 实测结果。

## 结论摘要

市面上的实现大致分为三代：

| 架构 | 文本/音频流 | 连续性来源 | 适合场景 |
|---|---|---|---|
| 分句请求式 TTS | 完整句子请求，音频响应可流式 | 句子内部上下文；句间靠播放器衔接 | 最容易落地，兼容传统离线 TTS |
| 双向增量 TTS | LLM 文本增量输入，同一上下文持续输出音频 | TTS 服务端保留本轮生成状态与韵律 | 当前主流低延迟级联方案 |
| 端到端 speech-to-speech | 音频 token 直接输入/输出 | 模型直接保留语调、情绪、重叠说话信息 | 最自然，但可控性、成本和本地部署门槛更高 |

对 Norma OS 最重要的判断是：**不要把“每个短句一个 TTS 请求”当作终态。** 这会降低首响，但每个请求都可能重新起调。短期应先完善“口语文本 + 自适应发声段 + 音频时钟队列 + 可打断状态机”；中期若试听仍像独立录音，应换成真正的 text-in/audio-out 双流模型。中文本地优先验证 CosyVoice 3 sidecar；云端可用 Cartesia Sonic 3.5 或其他上下文式 WebSocket TTS 做质量上限对照。

## 一、市场上的三种实现

### 1. 完整文本请求、流式音频响应

这类 API 收到完整文本后开始合成，但不用等整个音频文件生成完才播放。它解决的是 TTS 生成完成等待，不解决“LLM 整轮结束后才调用 TTS”的等待。Deepgram 明确建议首个音频字节到达后立即播放；其文档也提醒，不要把一个句子拆成多个独立请求，否则音高和表达的自然波动会造成割裂。[Deepgram TTS latency](https://developers.deepgram.com/docs/text-to-speech-latency)

如果只能使用这类模型，市场常见做法是：

- 缓存 LLM token，直到得到完整句子或稳定从句；
- 第一段优先合成并立即播放，后续段在前一段播放时预合成；
- 句内不拆请求；长句才在从句边界切；
- 所有 PCM 块排到同一音频时钟，不用 wall-clock timer 猜测播放结束。

Deepgram 给出的经验是语音助手约 50–100 字符、呼叫中心尽量用完整句子、长文用更大块；这不是中文的现成阈值，但说明行业优化目标是“足够语境下的最早稳定边界”，而不是越短越好。[Deepgram text chunking](https://developers.deepgram.com/docs/tts-text-chunking)

### 2. 真正的双向增量 TTS

这一类通过长连接同时接收增量文本和返回增量音频。关键不只是 WebSocket，而是**同一轮是否保留 TTS 生成上下文**。

- ElevenLabs 允许逐词或逐句发送文本，并用 chunk schedule 或 `auto_mode` 决定何时提交音频。官方把矛盾说得很清楚：过早提交会失去后文语境、边界不自然；等得更久则首响变慢。[Understanding audio streaming](https://elevenlabs.io/docs/eleven-api/concepts/audio-streaming)
- ElevenLabs 建议在完整句子处 `flush`，对长文还支持 `previous_text` / `next_text` 或相邻 request id 维持跨块韵律。[WebSocket realtime TTS](https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets/realtime-tts)、[TTS overview](https://elevenlabs.io/docs/overview/capabilities/text-to-speech)
- Cartesia 的 `context_id` 明确代表一条连续生成流；同一 context 的后续输入延续已有生成并维持韵律。一次对话回复用一个 context，被打断后新建 context。[Cartesia WebSocket TTS](https://docs.cartesia.ai/api-reference/tts/websocket)
- Amazon Polly 的双向流会在服务端重组多个 `TextEvent`，事件本身不必对齐标点；默认等待自然语言边界，强制 flush 会立即合成，但在句中 flush 可能损害发音和语调。[Polly bidirectional streaming lifecycle](https://docs.aws.amazon.com/polly/latest/dg/bidirectional-streaming-lifecycle.html)
- Google Cloud `StreamingSynthesize` 允许边发文本边收音频，同时仍建议每次输入包含终止完整句，以取得更好韵律。[Google Cloud TTS RPC reference](https://docs.cloud.google.com/text-to-speech/docs/reference/rpc/google.cloud.texttospeech.v1)
- Azure 的文本流式端点同样以首字节延迟和完成延迟分别观测，说明生产系统不会把“合成完成”当作“可以开始播放”的条件。[Azure TTS latency guide](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/how-to-lower-speech-synthesis-latency)

因此，一个成熟的 text stream coordinator 通常不是简单正则切句，而是：

1. 持续接收 LLM delta，并先完成 Markdown 清理、数字/缩写规范化等口语文本处理；
2. 完整词、短语和标点只作为候选提交点；
3. 达到最小上下文后，在强边界提交；短句默认继续等待并与后文合并；
4. 达到首响 deadline 或最大长度时，在最近的安全从句边界提交；
5. 轮末发送 flush/close，不能让尾句滞留；
6. 同一回复保持同一 generation context，不能每提交一段就重建模型状态。

也就是说，“切分”最好理解成**提交策略**，不是把一句回复变成互不相关的多个录音任务。

### 3. 端到端 speech-to-speech

OpenAI Realtime 与 Gemini Live 代表了托管式端到端路线：模型直接处理和生成音频，避免 ASR→文本 LLM→TTS 在中间丢失情绪、重音和口音，也减少串行组件延迟。[OpenAI Realtime](https://openai.com/index/introducing-gpt-realtime/)、[Gemini Live capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities)

Moshi 论文展示了更激进的 full-duplex 架构：用户与 Agent 的语音是并行 token 流，不依赖离散轮次，因此能建模重叠说话、插话和 backchannel；论文报告理论 160ms、实践约 200ms 延迟。[Moshi paper](https://arxiv.org/abs/2410.00037)

端到端的优势是自然度、打断和语音细节；代价则是：

- 业务需要的精确文本、审计、发音词典和逐段策略更难完全控制；
- 工具调用、长推理、输出安全检查仍需额外编排；
- 云服务带来成本、隐私和网络抖动；
- 本地模型的语言、硬件和产品成熟度仍不如级联方案稳定。

所以市场不是简单从级联“淘汰”到端到端。更现实的产品常保留两条路径：低延迟闲聊走原生语音模型，要求精确文本、工具流程或本地隐私时走级联管线。

## 二、播放与打断是独立的一半工程

即使 TTS 连续，播放器设计不当仍会出现断裂。

### 播放缓冲

市场共识是小型 jitter buffer，而不是收到一个网络包就立即以独立播放器播放。Fish Audio 建议先缓冲 2–3 个音频块，并在块间平滑衔接；Cartesia 官方示例则把每块按 `nextStartTime` 排到同一 `AudioContext` 时间轴。[Fish realtime streaming](https://docs.fish.audio/developer-guide/best-practices/real-time-streaming)、[Cartesia low-latency playback](https://docs.cartesia.ai/examples/tts-websocket-low-latency)

推荐的本地语义是：

- `queued`：已收到、未播放；
- `scheduled`：已排入音频设备，但用户未必听到；
- `played_ms`：以设备播放进度为准；
- `generation_done` 与 `playback_done` 分开；
- 缓冲低水位触发补充，高水位产生背压；不能无限积压已过时语音。

### Barge-in

真正的打断不是只调用 `player.stop()`，而是一组原子动作：

1. VAD/手动操作确认用户开始新输入；
2. 立即停止当前播放并清空尚未播放的队列；
3. 取消 TTS 和上游 LLM 生成；
4. 用实际 `played_ms` 截断对话历史，只保留用户真正听到的内容；
5. 新回复使用新的 generation/context id，丢弃旧 id 的迟到音频。

OpenAI WebRTC 会代管输出清空；WebSocket 模式要求客户端自行停播，并用播放追踪器按实际播放进度截断历史。[OpenAI Agents realtime guide](https://openai.github.io/openai-agents-python/realtime/guide/)、[OpenAI JS voice agents](https://openai.github.io/openai-agents-js/guides/voice-agents/build/)

Gemini Live 在 VAD 检测到打断后取消当前生成，但也明确要求客户端停止播放并清空本地队列。[Gemini Live capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities)

Azure Voice Live 将 `interrupt_response` 与 `auto_truncate` 分开配置，也说明“停止生成”和“让会话历史与实际听到的音频一致”是两个不同问题。[Azure Voice Live reference](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/voice-live-api-reference-2026-04-10)

## 三、开源模型与本地 macOS 中文

### CosyVoice 3：最值得做的本地双流实验

官方仓库称 Fun-CosyVoice 3 支持中文、18 种以上中文方言/口音、文本归一化、情绪/语速等指令，并支持 text-in/audio-out bi-streaming，报告最低约 150ms 延迟；其 0.5B 模型对本地实验也比大型端到端对话模型现实。[CosyVoice repository](https://github.com/FunAudioLLM/CosyVoice)

但官方部署路径明显偏 Python、CUDA、vLLM/TensorRT，仓库没有给出经过验证的 Apple Silicon 实时指标。因此对 Norma OS 的合理结论是“优先 POC”，不是“已证明可替换”：

- 先做独立 sidecar，不把 Python/CUDA 依赖塞进 Rust/GPUI 主进程；
- 在目标 M1/M2/M3/M4 档位分别测 TTFA、RTF、内存、持续 10 轮后的抖动；
- 必须测短确认词、连续短句、中文数字、中英混排和打断恢复；
- 若 Apple Silicon 无法稳定低于 1.0 RTF，则保留现有轻量 TTS 作为离线兜底。

### Fish Speech / Fish Audio：可作为中文云端或服务端对照

Fish Audio 官方 WebSocket 支持增量 text event、缓冲、flush 和增量音频，官方最佳实践建议缓存 5–10 个词、缓冲 2–3 个音频块。[Fish WebSocket TTS](https://docs.fish.audio/api-reference/endpoint/websocket/tts-live)

它能验证“真正双流是否解决当前连续性问题”，但其本地开源运行与云 API 不是同一部署承诺，不能把云端延迟直接外推到 Mac。

### Moshi：架构参照，不是当前中文替代方案

Moshi 官方仓库提供 MLX 与 Metal 路径，MLX 版本在 M3 MacBook Pro 上测试过，且客户端明确需要 echo cancellation 和积压补偿才能获得完整体验。[Moshi repository](https://github.com/kyutai-labs/moshi)

它适合验证 full-duplex、backchannel 和重叠说话的未来交互，但不能直接替换 Norma 当前“中文 Agent 文本必须精确、可展示、可调用工具”的 TTS seam。

## 四、Norma OS 推荐路线

### Phase A：不换模型，先把管线做对

当前实现会先收齐 provider delta，再把完整回复一次交给 sherpa-onnx `OfflineTts`；播放器虽能在同一 `AVAudioPlayerNode` 上 schedule buffer，但没有按实际播放进度回传 completion。[dialogue.rs](../../crates/agent/src/system/dialogue.rs)、[tts.rs](../../crates/voice_sherpa/src/tts.rs)、[macos.rs](../../crates/app/src/app/voice/playback/macos.rs)

建议先落地以下可复用接口：

```text
LLM delta
  -> SpokenTextNormalizer
  -> UtteranceCommitter（最小语境 + 安全边界 + deadline）
  -> TtsGeneration { turn_id, generation_id, segment_seq }
  -> PcmJitterQueue
  -> AudioClock / played_ms
```

具体策略：

- 显示文本与 `spoken_text` 分离；
- 短确认词若后面仍在快速到达，合并到下一发声段；
- 强标点是优先边界，逗号仅在超长或 deadline 到期时使用；
- 初始参数可从“最小约 8–12 个汉字、软目标 18–32、硬上限 50–60、首段额外等待不超过 200–400ms”起步，但最终由试听集调参；
- 播放时合成下一段；按 completion/设备时钟推进，不按 PCM 理论时长设 timer；
- 停播时清空队列，并让迟到的旧 generation 音频失效。

这一阶段能解决等待整轮、音频队列和打断一致性，但**无法从根本上消除独立 Matcha 请求之间的韵律重置**。

### Phase B：同一接口下做三路盲测

保持上面的 coordinator 与播放器不变，只更换 `StreamingTtsProvider`：

1. 现有 Matcha，作为隐私/离线基线；
2. CosyVoice 3 sidecar，验证本地中文真正 bi-streaming；
3. Cartesia Sonic 3.5 或同类云 WebSocket，作为连续性和 TTFA 上限对照。Sonic 3.5 官方列出中文支持，并用同一 context 延续生成韵律。[Sonic 3.5](https://docs.cartesia.ai/build-with-cartesia/tts-models/latest)

若本地 CosyVoice 达不到实时要求，可采用“本地 Matcha 默认 + 用户可选云端高质量”的产品策略，而不是让主链路依赖一个尚未验证的本地大模型。

### Phase C：端到端语音仅做独立探索

端到端 S2S 不应直接复用 `TtsClient` 接口，因为它改变的不只是 TTS，而是 turn detection、上下文、打断、工具调用和历史表示。应作为单独的 `RealtimeConversationProvider` POC，和级联路线并存。

## 五、验收指标

不要只测“合成耗时”。至少记录：

- `LLM first_delta`、`first_committable_boundary`、`TTS first_audio`、`speaker first_played`；
- TTFA 的 P50/P95，以及播放 underrun 次数；
- 轮末尾音是否完整、首尾静音、段间静音分布；
- 打断到静音的时间、打断后旧音频泄漏量、history 截断误差；
- 盲听 MOS：自然度、短句连续性、语气一致、中文可懂度；
- CPU/GPU、峰值内存、RTF、10–30 分钟热运行稳定性。

固定试听集至少包括：

- “好的。正在处理。稍等一下。”这类连续短句；
- 一句很短、下一句较长的回复；
- 中文数字、日期、金额、英文产品名和路径；
- 问句接解释、列表转口语；
- 播放 20%、50%、80% 时打断；
- 连续两轮要求语气延续的对话。

## 六、最终判断

当前问题包含两层：Matcha 的对话韵律能力上限，以及管线把整轮文本做成单块音频。先改管线是必要的，因为无论未来接哪家模型，提交策略、播放时钟、背压和打断语义都必须存在；但若目标是明显优于“分段朗读”，终态应是**同一回复上下文内的双向增量 TTS**，而不是不断微调正则切句。

对 Norma OS，最稳妥的技术路线是：

1. 先完成模型无关的 streaming coordinator 与播放/打断状态机；
2. 同时用云端 context TTS 验证体验上限；
3. 用 CosyVoice 3 sidecar 验证 Apple Silicon 中文实时性；
4. 保留 Matcha 作为离线 fallback；
5. full-duplex S2S 另立实验，不绑住当前产品交付。

## 一手来源索引

- OpenAI：[Realtime 发布说明](https://openai.com/index/introducing-gpt-realtime/)、[Realtime Agents 指南](https://openai.github.io/openai-agents-python/realtime/guide/)
- ElevenLabs：[audio streaming 原理](https://elevenlabs.io/docs/eleven-api/concepts/audio-streaming)、[WebSocket TTS](https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets/realtime-tts)
- Cartesia：[WebSocket contexts](https://docs.cartesia.ai/api-reference/tts/websocket)、[Sonic 3.5](https://docs.cartesia.ai/build-with-cartesia/tts-models/latest)
- Deepgram：[TTS latency](https://developers.deepgram.com/docs/text-to-speech-latency)、[text chunking](https://developers.deepgram.com/docs/tts-text-chunking)
- Google：[Gemini Live capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities)、[Cloud TTS StreamingSynthesize](https://docs.cloud.google.com/text-to-speech/docs/reference/rpc/google.cloud.texttospeech.v1)
- Microsoft：[Azure TTS latency](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/how-to-lower-speech-synthesis-latency)、[Voice Live API](https://learn.microsoft.com/en-us/azure/ai-services/speech-service/voice-live-api-reference-2026-04-10)
- AWS：[Polly bidirectional streaming](https://docs.aws.amazon.com/polly/latest/dg/bidirectional-streaming.html)、[stream lifecycle](https://docs.aws.amazon.com/polly/latest/dg/bidirectional-streaming-lifecycle.html)
- FunAudioLLM：[CosyVoice 官方仓库](https://github.com/FunAudioLLM/CosyVoice)、[CosyVoice 2 论文](https://arxiv.org/abs/2412.10117)、[CosyVoice 3 论文](https://arxiv.org/abs/2505.17589)
- Fish Audio：[WebSocket TTS](https://docs.fish.audio/api-reference/endpoint/websocket/tts-live)、[实时流式最佳实践](https://docs.fish.audio/developer-guide/best-practices/real-time-streaming)
- Kyutai：[Moshi 官方仓库](https://github.com/kyutai-labs/moshi)、[Moshi 论文](https://arxiv.org/abs/2410.00037)
