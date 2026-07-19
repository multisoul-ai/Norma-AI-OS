# ADR-0026: Built-in ASR providers 归属 `asr`

- 日期：2026-07-19
- 状态：Accepted

## 背景

旧边界把 ASR 模型元数据放在 `asr`，却把 Sherpa 识别器、麦克风采集、VAD 和识别事件适配都放在 `voice_sherpa`。当 Azure Speech 成为第二个 provider 时，这种按实现技术命名的分层会迫使云端识别继续进入 `voice_sherpa`，也会让 Settings 和业务流程感知各 provider 的细节。

## 决策

- `asr` 统一拥有 provider registry、能力描述、规范化识别事件、`RecognitionPipeline`，以及内置的 Local Sherpa 和 Cloud Azure provider。
- `asr` 保持零 GPUI、零 Storage、零 Keychain；provider 可以依赖各自所需的识别引擎或网络客户端。
- `voice_input` 统一拥有麦克风权限、设备枚举、采集、重采样、PCM 流与共享 VAD，并保持 provider 中立。
- `app` 持久化非敏感 provider profile 和不透明 credential reference；API key 只写入 macOS Keychain。
- Local 是默认 provider。Azure 必须经过 Draft → Validate → Ready，再由用户明确激活；失败时不得静默回退 Local。
- Azure endpoint 由 region 推导，输入统一为 16 kHz mono PCM，当前只发布 final 结果；单次语音命令最多采集 60 秒。
- `voice_sherpa` 暂时作为既有调用方的兼容门面，承接 Sherpa TTS 和连续对话适配，不再拥有音频输入或 provider 选择。
- 职责不清且只剩一个模型常量被消费的旧 `voice` crate 被移除。

## 结果

新增 provider 时，识别协议和业务流程不再复制；Local 原有默认路径保持不变。代价是迁移期内仍保留 `voice_sherpa` 兼容层，后续应在调用方全部转向 provider-neutral API 后再单独删除。

本决策取代 ADR-0004/0009 中“`asr` 必须零 Sherpa/网络、所有识别器适配必须属于 `voice_sherpa`”的部分；其余模型安装和 TTS 边界继续有效。
