# 远程 TTS 供应商实现集中在 tts crate

我们决定不为 MiniMax 及后续每个 TTS 厂商新建 `tts_<vendor>` crate，而是将供应商协议、目录、错误映射和 Adapter 收口在 `crates/tts/src/providers/<vendor>/`，由 `tts` 对外提供统一的供应商、选择与回复级流式合成接口。这使新增厂商的变更保持在 TTS 能力模块内，App 不需逐一依赖供应商 crate。作为代价，`tts` 不再保持零网络依赖；新的强制边界是 `tts` 仍保持零 GPUI、零 Storage 和零 Sherpa，凭据保存与前台编排继续属于 App，Sherpa 本地引擎 Adapter 继续属于 `voice_sherpa`。
