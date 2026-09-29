# 火山方舟豆包语音 API 与 Norma OS 桌面接入

> 核查日期：2026-09-29。依据用户方舟控制台截图、[豆包语音官方 ASR 文档](https://docs.volcengine.com/docs/DoubaoVoice/LargemodelstreamingautomaticspeechrecognitionAPI?lang=zh)、[官方 TTS 文档](https://docs.volcengine.com/docs/DoubaoVoice/bidirectional-streaming-text-to-speech-websocket?lang=zh)、仓库已有 `services/voice/plan.mjs` 和本机真实接口测试。本文不含密钥值。

## 结论

**用户提供的是火山方舟 API Key，可用于截图中的 ASR 2.0、方舟 Chat 2.0 和 TTS 2.0。** 本机使用同一把 Key 已得到 ASR 2.0 的有效二进制配置响应、方舟 `doubao-seed-2.0-mini` 的 HTTP 200 回复、TTS 2.0 的 HTTP 200 和非空 PCM 音频。此前在 Seeduplex 3.0 端点得到的 HTTP 401，只能证明这把 Key 不适用于该端点，不能推断它无法用于用户所选的 2.0 产品。

截图列出的资源与地址：

| 服务 | 地址 | 鉴权／模型 |
| --- | --- | --- |
| 流式 ASR 2.0 | `wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async` | `X-Api-Key`；`X-Api-Resource-Id: volc.seedasr.sauc.duration` |
| 豆包 Chat 2.0 | `https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions` | `Authorization: Bearer <方舟 Key>`；`doubao-seed-2.0-mini` |
| 流式 TTS 2.0 | `https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional` | `X-Api-Key`；`X-Api-Resource-Id: seed-tts-2.0` |

方舟控制台明确提示不要使用通用 `https://ark.cn-beijing.volces.com/api/v3` 地址，否则会产生额外费用。桌面实现使用截图对应的 `/api/v3/plan/...` 语音路由和 `/api/plan/v3/...` Chat 路由。

## API 能力区分

| 产品 | 实际能力 | 桌面用途 |
| --- | --- | --- |
| ASR 2.0 | 双向流式 WSS，16 kHz PCM16，经 V3 gzip 二进制帧上传，返回增量／明确最终转写 | 持续收音、检测插话和生成用户话轮 |
| 方舟 Chat 2.0 | 文本对话与函数调用；可用同一方舟 Key | 复用 Norma 人设、项目信息和工具声明 |
| TTS 2.0 | 流式文本转音频，支持 PCM 24 kHz | 边收到音频边播报，插话时取消与清空 |
| Seeduplex 3.0 | 单一服务的原生端到端语音对话与 Function Calling | 当前方舟 Key 在其端点返回 401；本次不接入 |

**ASR 2.0 与 TTS 2.0 不是一个原生全双工语音模型。** Norma 桌面将 ASR、Chat、TTS 编排成一条可同时收音和播音、可插话的会话链路。其响应时间包含 ASR 断句、Chat 生成和 TTS 首音，不能沿用 Seeduplex 3.0 的延迟宣传值。

## 桌面实现与复用

```text
麦克风 + macOS Voice Processing AEC
  → 20 ms PCM 采集，100 ms 聚合上传 ASR 2.0
  → 最终转写 → 方舟 Chat 2.0（现有 Norma 提示词、项目和工具）
  → 回复文本 → TTS 2.0 流式 PCM → 可中断播放器
  → 函数调用 → 现有 NormaToolRuntime / AgentCardAdapter → 真实回执
```

桌面旧 Voice Commands 仍走已有本地／Azure ASR → Norma Agent → 本地／MiniMax TTS。豆包 2.0 模式只复用麦克风、AEC、PCM 播放、Keychain、提示词和工具执行边界，不改变旧模式配置。iOS 云语音网关的 [`plan.mjs`](../../services/voice/plan.mjs) 已使用同一组三段式方舟路由，可作为协议参考；桌面不依赖该网关运行。

低延迟措施：ASR 上传批次为 100 ms；Chat 选用 `doubao-seed-2.0-mini` 并关闭 thinking；TTS 逐块读取响应并立即排队；新 ASR 话轮取消旧 Chat/TTS 和旧 PCM。工具回执按 `call_id` 持久去重，已接收的项目任务不会因插话取消。

## 证据与未完成验收

- 本机 Keychain：service `com.norma-ai.os.agent-provider`、account `doubao-ark-e2e`；仓库只保存定位信息，不含密钥明文。
- 真实服务测试 `cargo test -p app --test doubao_real -- --ignored --nocapture`：ASR 配置确认、Chat 非空回复及 TTS 非空 PCM 全部通过。该测试未使用麦克风。
- 假上游桌面 E2E：分别模拟 ASR WSS、Chat HTTP 函数调用、TTS HTTP 音频流；覆盖模式切换、提示词、工具去重和 PCM 上传。
- 待真实 `.app` 进行麦克风、耳机与内置扬声器的回声和插话体验验收，并测量停语到首音的 p50／p95。真实服务鉴权通过不等于硬件低延迟已证实。
