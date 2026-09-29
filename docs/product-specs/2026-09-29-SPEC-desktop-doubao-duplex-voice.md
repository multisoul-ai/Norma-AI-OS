# 桌面 Voice 接入火山方舟豆包 2.0 SPEC

状态：`ASR 2.0／方舟 Chat／TTS 2.0 适配已实现；本机服务鉴权与假上游 E2E 通过；真实麦克风和扬声器延迟待验收` · 2026-09-29

## 目标与边界

Norma OS 桌面保留原有 Voice Commands（单次指令和持续聆听）及其 ASR → Norma Agent → TTS 配置。新增独立的 **Doubao 2.0 Voice** 模式，使用用户提供的火山方舟 Key：流式 ASR 2.0、方舟 `doubao-seed-2.0-mini` 对话、流式 TTS 2.0。它们是三个服务，由桌面会话协调并持续收音、支持播报时插话；这不是 Seeduplex 3.0 原生端到端全双工模型。

Settings → Voice 顶部切换两种模式。`⌘E` 启停当前模式；切换模式或项目时结束旧通话，保留两侧配置。豆包模式复用现有 Norma 人设、当前项目、Agent 名册、画布上下文、工具定义及桌面工具 Adapter；普通对话不提交第二个 Norma 模型回合，项目操作仍只能由已有工具边界执行。

## 用户体验

- 豆包模式单独保存方舟 Key 到 macOS Keychain，设置页不展示明文，也不要求旧模式的 ASR、Agent Provider 或 TTS 已配置。
- 用户可持续讲话；ASR 增量识别用于检测新话轮，收到明确最终转写后发起一次方舟对话。播报中插话立刻停止旧音频和旧对话请求，麦克风持续工作。
- 方舟提出工具调用时，客户端必须以用户真实最终转写、当前项目、工具名和参数做授权；“可以／继续”只对上一轮唯一待确认动作有效。缺少转写或目标含糊时拒绝写操作。
- 工具返回真实回执，并按 `call_id` 与持久操作记录去重。`accepted`／`queued` 只表示已接收；Agent 的完成／失败播报必须等待可信终态。插话不取消已提交的项目任务。
- 豆包会话不自动写入旧项目聊天历史，也不保存原始音频；掉线显示错误，不静默回退到旧模式。

## 技术实现

```text
macOS 麦克风 + Voice Processing AEC
  → voice_input：16 kHz 单声道 PCM16，20 ms 采集块，有界队列
  → ASR 2.0：每 100 ms 聚合成 V3 gzip 二进制帧，WSS bigmodel_async
  → 最终转写 → 方舟 Chat：doubao-seed-2.0-mini，复用 Norma 提示词和函数定义
  → 普通回复 → TTS 2.0：HTTP 流式 PCM 24 kHz → 可中断播放器
  → 项目工具 → 原 NormaToolRuntime + AgentCardAdapter → 回执返回方舟 Chat
```

- ASR：`wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async`，`X-Api-Resource-Id: volc.seedasr.sauc.duration`。
- Chat：`https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions`，模型 `doubao-seed-2.0-mini`，关闭 thinking，以较短回复降低等待时间。
- TTS：`https://openspeech.bytedance.com/api/v3/plan/tts/unidirectional`，`X-Api-Resource-Id: seed-tts-2.0`，逐块播放 24 kHz PCM。
- ASR 与播放同时运行。新话轮取消旧 Chat/TTS、清空旧 PCM；播放器用 generation 拒绝迟到音频。工具一经提交独立完成，不因插话丢失持久回执。
- 会话独立于原有互斥 DialoguePhase。网络、Keychain、编解码、模型和音频播放均在后台，不阻塞 GPUI 前台。
- GPT-Live 不在本次实现范围；未来需要其独立协议适配，不能假设与 ASR/TTS 2.0 共用单一上游事件流。

## 验收

1. 设置与旧模式：升级默认仍为旧链路；切换豆包、保存 Key、重载及 `⌘E` 启停通过应用级 E2E 和真实 App 视觉检查。
2. 假上游 E2E：分别模拟 ASR 2.0 二进制 WSS、方舟 Chat 工具调用、TTS 2.0 流式 PCM；验证提示词含项目、工具只执行一次、真实 `node_id` 回执、旧 DialoguePhase 不启动。
3. 聚焦测试：二进制 ASR 帧、PCM 播放插话与迟到音频、方舟请求中的人设与工具、凭据只存 Keychain，以及 Agent 可信终态播报。
4. 真实服务：本机 Keychain 的用户方舟 Key 分别通过 ASR 2.0 配置确认、方舟 Chat 回复和 TTS 2.0 非空 PCM。不得将它误用作 Seeduplex 3.0 鉴权。
5. 硬件专项：在真实 `.app` 上用耳机与 Mac 内置扬声器／麦克风检查回声、插话、首音延迟 p50／p95。自动化服务验收不等于硬件体验通过。

## 不在范围内

不改旧 Single／Continuous 语义，不把豆包 2.0 注册为旧 ASR/TTS 的全局 Provider，不实现 Seeduplex 3.0 或 GPT-Live，不保存通话音频或自动合并旧聊天历史。
