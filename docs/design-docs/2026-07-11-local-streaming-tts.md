# 本地流式 TTS 决策

## 决策

语音对话第一阶段固定使用本地 Matcha。回复文本按语义边界分段：完整短句最多等待 250ms 与后续文本合并；首次可播 PCM 累积到 120ms 后启动播放；最终不足 120ms 的尾段直接播放，不插入静音。

LLM 文本增量、Matcha PCM 与播放器完成事件均为独立阶段。界面保留原始显示文本，送入 TTS 前仅去除 Markdown 结构、代码块与 URL 的朗读噪音。macOS 以 AVAudioPlayerNode 的 `DataPlayedBack` 回调确认播放结束，不能用 PCM 理论时长计时。

用户确认打断时，取消当前 LLM 流、TTS worker 与音频队列；CardTask 不取消。TTS 失败保留文本、显示错误并回到 Listening。

## 取舍

Matcha 不能维持跨调用的声学状态，因此本阶段的“连续”只保证同一回复内的语义分段和无人工静音拼接。云端 TTS 只通过 `TtsClient::synthesize_stream` 留出能力边界；当前不接入，也绝不自动回退到本地。

## 验证

- 单元测试覆盖 Markdown 口语化、短句合并、120ms 预缓冲和显式播放完成。
- 真实模型验收使用 Matcha WAV 语料，盲听连续短句、短长组合、问答解释、混合中英数字路径和打断。
