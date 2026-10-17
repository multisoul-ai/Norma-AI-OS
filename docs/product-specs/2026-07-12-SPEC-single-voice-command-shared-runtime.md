# 单次语音指令与持续聆听共享运行时 SPEC

> 历史：见 git 提交。原文路径：`docs/exec-plans/2026-07-12-single-voice-command-shared-runtime.md`。

产品行为以本文和 [ADR 0013](../adr/0013-share-the-voice-turn-runtime-across-interaction-modes.md) 为准。代码中已有 `SingleVoiceCommand`。

## Problem Statement

当前语音交互把旧听写实现为独立的一次性录音、转写和焦点文本写入链路，而持续聆听另有 PCM、VAD、流式 ASR、Agent 和 TTS 链路。两条链路虽然使用同一模型选择和相同入口，却有重复的麦克风生命周期、状态、错误恢复和测试面；旧听写也无法得到 Agent 执行与语音回复。

用户需要保留两种互斥的交互模式，但将旧听写升级为“单次语音指令”：以快捷键定义一次语音回合，复用持续聆听的识别、Agent、工具、TTS 和实时文本反馈主流程；持续聆听继续保留 VAD 驱动的长会话能力。

## Solution

Voice Settings 提供互斥的“单次语音指令”和“Continuous Listening”。两者共享当前 ASR 模型、输入设备、流式 partial、最终文本、Agent Provider、工具执行和 TTS 回复。`⌘E` 是唯一语音入口。

单次语音指令不使用 VAD。Hold 在按住 `⌘E` 期间收音，并在松开时结束输入、flush 一个 Final Transcript；Toggle 在第一次按下开始、第二次按下结束。Final Transcript 进入与持续聆听完全相同的 Agent/工具/TTS 回合。它不会把文本直接写入焦点卡片，也不会维持长会话。

Continuous Listening 保持长会话与 VAD 分段。VAD 每取得一个 Final Transcript 就开始同一 Agent/工具/TTS 回合；TTS 完成或被打断后恢复 Listening。两种模式均显示 partial 文本。

## User Stories

1. 作为 Norma OS 用户，我想在单次语音指令和 Continuous Listening 之间选择其一，以便按任务选择一次命令或多轮语音会话。
2. 作为单次语音指令用户，我想按住 `⌘E` 说话、松开提交，以便明确控制一轮输入的边界。
3. 作为单次语音指令用户，我想选择 Toggle，以便不必持续按住快捷键。
4. 作为 Toggle 用户，我想第一次 `⌘E` 开始、第二次 `⌘E` 提交，以便行为可预测。
5. 作为单次语音指令用户，我想在说话时看到 partial 文本，以便确认麦克风和识别正在工作。
6. 作为单次语音指令用户，我想在结束输入后让 Agent 执行工具并以 TTS 回复，以便完成一次完整语音任务。
7. 作为单次语音指令用户，我想在 Agent Thinking 时再次按 `⌘E` 取消旧任务并立即开始新录音，以便修正意图无需等待。
8. 作为单次语音指令用户，我想在 TTS 播放时再次按 `⌘E` 打断回复并立即开始新录音，以便自然地插话。
9. 作为 Continuous Listening 用户，我想用 `⌘E` 启动或停止长会话，以便明确麦克风的持续使用时间。
10. 作为 Continuous Listening 用户，我想由 VAD 自动切分多段话语，以便连续说话时不必每句操作快捷键。
11. 作为 Continuous Listening 用户，我想在 TTS 中按 `⌘E` 仅打断回复并回到 Listening，以便不中断整个会话。
12. 作为 Continuous Listening 用户，我想在 Agent Thinking 时按 `⌘E` 停止整个会话，以便明确取消连续任务。
13. 作为语音用户，我想两种模式使用同一识别模型、输入设备、Agent Provider、工具权限和 TTS 配置，以便切换模式不会改变能力或配置。
14. 作为语音用户，我想切换模式、模型或输入设备时停止所有活跃语音、Agent 和 TTS 工作，以便旧配置绝不处理新音频。
15. 作为语音用户，我想配置改变后必须再次按 `⌘E` 才能启动，以便麦克风绝不被自动重启。
16. 作为升级用户，我想旧 `dictation` 模式和 Hold/Toggle 偏好自动迁移到单次语音指令，以便保留已有操作习惯。
17. 作为升级用户，我想旧听写不再把文本直接写入焦点卡片，以便语音入口始终走统一的 Agent 回合。
18. 作为维护者，我想单次和连续模式仅在回合边界与会话保持策略上不同，以便删除重复录音、识别和执行代码。
19. 作为维护者，我想单次模式的流式识别无需初始化 VAD，以便减少不需要的资源与状态。
20. 作为 QA 工程师，我想用确定性 ASR、LLM 和 TTS 替身覆盖全部快捷键与状态转换，以便日常测试不依赖真实麦克风或模型。

## Implementation Decisions

- `VoiceInteractionMode` 由 `Dictation` 更名为 `SingleVoiceCommand`；持久化值由 `dictation` 迁移为 `single_voice_command`。
- `voice.dictation.trigger` 迁移为 `voice.single_voice_command.trigger`，并保留 Hold / Toggle 的含义。新用户默认单次语音指令 + Hold。
- 所有语音激活使用固定、只读的 `⌘E`；不再使用 `⌘⇧V`，也不把语音入口做成可编辑 app shortcut。
- 单次语音指令与 Continuous Listening 仍为互斥模式。模式切换只改变回合边界和回复后的会话策略，不改变 ASR、Agent、工具或 TTS 的选择。
- 旧一次性听写控制器、焦点文本写入分支和专属录音 finalizer 从产品运行时移除；不保留兼容执行路径。
- 识别运行时拆分为共享 PCM/流式 ASR 处理和可替换的 final 边界策略。单次语音指令由 Hold/Toggle 的结束请求 ASR `end_utterance`；Continuous Listening 仅由 VAD SpeechEnd 请求它。
- 共享的 Final Transcript 必须进入既有 Agent 回合、工具执行和 TTS 回复编排。单次模式的播放完成后进入 Idle；连续模式的播放完成或 TTS 打断后回到 Listening。
- 单次模式在 Thinking 或 Speaking 收到 `⌘E` 时取消现有 Agent/TTS 并开始一轮新录音。连续模式在 Speaking 收到 `⌘E` 时只中断回复，在 Thinking 收到 `⌘E` 时停止长会话。
- 模式、模型或输入设备变更必须统一取消录音、PCM、ASR worker、Agent 回合与 TTS；任何异步启动结果必须通过 generation/attempt 检查，避免陈旧任务重新安装运行时。
- partial 文本来自共享流式 ASR 事件，并在两种模式的底栏显示；它不是 Final Transcript，也不得启动 Agent 或工具。
- ASR、Dialogue 与 Sherpa 领域层继续零 GPUI；权限、模型验证、识别 join、Agent/LLM 和 TTS 的阻塞工作留在后台，前台仅应用状态与 UI 副作用。

## Testing Decisions

- 所有新增单测遵守仓库场景/数据构造/执行过程/预期结果注释结构；每个断言含明确说明，关键保留与取消状态逐一正负断言。
- `voice_sherpa` 在公开的流式 ASR seam 验证单次边界：PCM 产生 partial，显式结束产生至多一个 Final，且不构造或调用 VAD；现有 VAD 管线继续验证连续分段。
- `dialogue` 只验证与模式无关的 `Listening → Thinking → Speaking → Idle/Listening` 状态契约，不复制 app 的快捷键测试。
- app 单测验证快捷键动作、Hold/Toggle、Thinking/TTS 打断、取消 generation、Settings 迁移，以及单次和连续模式共享同一 Agent/TTS 回合入口。
- `crates/e2e` 是主要用户行为 seam：通过 Settings、`⌘E` 及确定性 ASR/LLM/TTS 替身验证两模式、升级迁移、无 VAD 单次边界、partial 状态、配置切换取消和不会自动重启。
- 真实模型与麦克风只保留为 ignored L3 smoke；报告必须与确定性 L1/L2 证据区分。

## Out of Scope

- 新的识别模型、模型安装机制、Agent Provider 或 TTS Provider。
- 自由配置语音快捷键、独立语音总开关或后台自动重启麦克风。
- 在单次语音指令中引入 VAD、沉默自动提交或多轮会话记忆。
- 将 Final Transcript 再写回焦点 Terminal、Agent 或其他文本卡片。
- 改变 Continuous Listening 的 VAD 算法、阈值或回声抑制策略。

## Further Notes

- 该 SPEC 取代旧“Dictation 直接写入焦点且不进入 Agent/TTS”的产品契约；历史 ADR-0006 与 ADR-0007 应标记为已被 ADR-0013 取代。
- 核心架构取舍记录在 ADR-0013：两种模式共享语音回合运行时，但保留不同的回合边界和会话保持语义。
