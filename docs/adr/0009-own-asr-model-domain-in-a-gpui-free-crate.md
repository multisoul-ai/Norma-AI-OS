# 由独立零 GPUI crate 拥有识别模型领域

新建 `crates/asr` 拥有识别模型身份、模型库、能力、安装清单、校验和选择规则；它不依赖 GPUI、Storage 或 Sherpa。`voice_sherpa` 只负责将 `asr` 提供的可用模型适配为 Sherpa recognizer，app 在后台调度下载并复用 `asr` 的安装与校验逻辑。这个分层避免将引擎文件布局泄漏到 Settings，也不把文件生命周期塞进 `dialogue::AsrClient`。
