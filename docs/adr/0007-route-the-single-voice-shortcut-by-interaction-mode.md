# 由语音交互模式路由唯一快捷键

> 已被 [ADR-0013](0013-share-the-voice-turn-runtime-across-interaction-modes.md) 取代。

Voice Settings 不保存独立的启用开关，保留 `⌘⇧V` 作为唯一语音入口，并由当前语音交互模式决定路由。听写支持按住或切换两种触发方式；持续聆听用按下启动/停止长会话，在 TTS 播放中则优先打断回应但保持会话。这取代同时表达模式与开关的 `realtime_enabled` 布尔语义。
