# 语音设置 · 高保真原型（对齐现有配置）

单文件原型：`voice-settings.html`，浏览器直接打开。右上角可切浅/深色、重置演示。

## 依据的代码与文案
- 对话模式：`crates/app/src/settings/voice/state/mode.rs`（legacy / doubao / gpt_live）。
- 语音指令：识别来源（Local Sherpa / 本地精准 A1 / Cloud Azure）、单次或持续聆听、按住或切换、麦克风。A1 仅用于单次模式（`supports_local_a1`）。
- 理解模型：智能体供应商 Ollama 或 OpenAI 兼容（`crates/agent` 的 `ProviderProtocol`）。
- 语音回复：TTS 供应商 On-device（Matcha / Kokoro 等本机模型）或 MiniMax（Speech 2.8 Turbo / HD，账户音色）。
- 豆包 2.0：火山方舟 API Key，音色来自 `crates/tts/src/providers/doubao/voices.json`。
- GPT-Live：OpenAI Key，音色 Marin / Cedar（`state/gpt_live.rs`）。
- 回复偏好：智能体任务播报、语音光晕。

## 已移除（代码中不存在）
停顿时长、语速、播报内容、音色试听。

## 待核对
- 本机模型的显示名（Matcha 中文 / 中英、Kokoro 多语）为原型简称，未与 UI 文案逐字核对。
- MiniMax 音色由账户拉取，原型只放占位。
- 「试一句」只做本地规则分类的演示，不接真实派发。
