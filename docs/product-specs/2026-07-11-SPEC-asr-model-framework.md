# 可扩展 ASR 模型框架与 Zipformer Streaming ZH SPEC

## Problem Statement

当前 Norma OS 的本地语音识别与 Paraformer 的文件目录和 Sherpa 配置直接绑定。用户在 Voice Settings 中看到的是模型路径而不是可理解的识别模型；听写、持续聆听、Agent Provider 和工程配置也混在一起，缺少清晰的语音交互模式。

这导致每次新增识别模型都需要修改 Settings、路径解析和多处 recognizer 配置，模型安装与用户选择没有明确边界。因此不能安全地接入 Zipformer Streaming ZH，也不能保证 Paraformer 在切换和失败场景中继续可用。

## Solution

先建立一个引擎无关、零 GPUI 的 ASR 领域框架，把识别模型的身份、受控模型库、能力、安装清单、校验、可用性和当前选择建模为独立概念。Settings 只保存不可变模型 ID，模型安装以 Application Support 中的文件系统清单作为唯一事实来源。

先把现有 Paraformer 作为第一个模型完整迁入新框架，验证模型库、安装、选择、Settings 和运行时链路；再以数据驱动方式加入 Zipformer Streaming ZH 与 Sherpa Transducer 适配。

Voice Settings 将以“语音交互”为顶层概念，提供互斥的听写模式和持续聆听模式。两种模式共享麦克风和当前识别模型，由 `⌘⇧V` 作为唯一入口，无独立总开关。Agent/LLM/TTS 保留在同一 Voice 页面，但按当前模式渐进展开。

## User Stories

1. As a Norma OS user, I want to choose an interaction mode, so that voice input behaves as dictation or as a continuous Agent conversation according to my current task.
2. As a dictation user, I want final recognized text written directly into the focused input target, so that I can enter text without invoking an Agent.
3. As a dictation user, I want to use hold-to-talk with `⌘⇧V`, so that recording ends naturally when I release the shortcut.
4. As a dictation user, I want to use toggle-to-talk with `⌘⇧V`, so that I can dictate without holding the keys down.
5. As a continuous listening user, I want `⌘⇧V` to start and stop a long-lived voice session, so that I control when the microphone is continuously active.
6. As a continuous listening user, I want `⌘⇧V` to interrupt TTS while preserving the session, so that I can barge in without rebuilding the whole conversation.
7. As a privacy-conscious user, I want voice interaction to have no independent master switch, so that the microphone only starts through my explicit shortcut action.
8. As a voice user, I want dictation and continuous listening to share one recognition model, so that switching modes does not unexpectedly load another model.
9. As a voice user, I want to see recognition models by friendly name, language, streaming capability and download size, so that I can choose without understanding ONNX files.
10. As a Chinese-speaking user, I want Zipformer Streaming ZH available as a local streaming model, so that I can improve Chinese recognition quality.
11. As an existing Paraformer user, I want Paraformer to remain available in the new model library, so that I can compare models and recover from a Zipformer problem.
12. As a new user, I want the model selector to explain that no model is installed, so that I know why `⌘⇧V` cannot start recording yet.
13. As a new user, I want to install a supported model directly from Voice Settings, so that I do not need to select a model directory manually.
14. As a user installing a model, I want visible download progress and a retryable error, so that a network interruption is understandable and recoverable.
15. As a user, I want an uninstalled model action to mean install rather than select, so that the current model never points at unavailable files.
16. As a user, I want an installed and validated model to become selectable, so that every selectable model can actually be loaded.
17. As a user, I want a successfully installed model to be selected only after runtime loading succeeds, so that a bad model does not break voice interaction.
18. As a user switching models, I want the active recording or continuous session stopped cleanly, so that old and new recognizers never process the same audio.
19. As a user switching to a broken model, I want the previous model retained, so that voice interaction remains usable.
20. As a user, I want to delete an unused installed model, so that I can reclaim disk space.
21. As a user, I want deletion of the current model blocked until I switch, so that Settings cannot create an invalid current selection.
22. As a user, I want model versions to change only through explicit installation and selection, so that an application update never silently replaces recognition weights.
23. As a user, I want model files stored under Norma OS application data, so that installation locations are predictable and managed consistently.
24. As a user, I want failed and partial installs isolated from ready models, so that retrying a download cannot corrupt a working recognizer.
25. As an upgraded user, I want my Agent Provider, LLM endpoint, model name and API key retained, so that the new ASR architecture does not erase unrelated voice conversation configuration.
26. As an upgraded user, I accept reselecting a voice interaction mode and reinstalling an ASR model, so that the new architecture does not carry permanent legacy path and boolean branches.
27. As a Voice Settings user, I want microphone and recognition model controls always visible, so that shared voice input configuration is easy to find.
28. As a dictation user, I want dictation trigger settings expanded and Agent configuration summarized, so that the page remains focused on my active mode.
29. As a continuous listening user, I want Agent Provider, LLM and TTS configuration expanded on the same page, so that the strongly related pipeline can be configured together.
30. As an advanced user, I want endpoints, API keys, trace and diagnostics placed in a collapsed advanced section, so that common voice configuration stays compact.
31. As a maintainer, I want recognition model identity and installation lifecycle independent of GPUI and Sherpa, so that the domain remains testable and reusable.
32. As a maintainer, I want Paraformer and Zipformer to share one recognizer factory, so that model-specific differences are not duplicated across session paths.
33. As a maintainer, I want a new supported model to appear through a curated catalog entry and runtime adapter, so that Settings does not gain model-specific branches.
34. As a maintainer, I want model IDs bound to immutable versioned artifacts, so that selection, manifests and hashes always refer to the same bytes.
35. As a maintainer, I want installation state derived from an installation manifest and required files, so that a database cannot claim missing files are installed.
36. As a maintainer, I want the model installer to consume an injected archive rather than perform network I/O, so that installation safety is deterministic and independently testable.
37. As a maintainer, I want application code to schedule blocking download, extraction and model loading off the GPUI foreground path, so that Settings and Canvas remain responsive.
38. As a release owner, I want download URLs and SHA-256 values pinned in the curated catalog, so that model installation has a controlled supply-chain boundary.
39. As a release owner, I want models downloaded by explicit user action rather than bundled in the app, so that application size stays manageable and unclear model redistribution rights are respected.
40. As a release owner, I want Zipformer marked recommended only after accuracy and performance evaluation, so that recommendation is based on product evidence.
41. As a QA engineer, I want fixed Chinese command WAV files and labels, so that Paraformer and Zipformer can be compared repeatably.
42. As a QA engineer, I want user-observable Settings, shortcut, installation, switching and fallback flows covered by e2e tests, so that the product contract is verified above implementation details.
43. As a performance engineer, I want model loading, first partial, final transcript, CPU and peak memory recorded on target Apple Silicon hardware, so that regressions are visible before release.
44. As a future maintainer, I want the domain boundary capable of accommodating another recognition engine without a plugin system today, so that current scope remains small without leaking Sherpa into user settings.
45. As a future TTS maintainer, I want TTS model management explicitly deferred, so that ASR is delivered without inventing a premature all-model framework.

## Implementation Decisions

- A new zero-GPUI ASR domain module will own model identity, the curated model library, capability metadata, installation manifests, installation validation, availability and selection rules.
- The ASR domain will not depend on the dialogue contract, Sherpa adapter, storage layer or application UI.
- The existing dialogue ASR client remains the engine-neutral PCM-to-partial/final boundary used by both voice interaction modes.
- Recognition model IDs are immutable and versioned. The user-facing display name omits version detail, while installation manifests and Settings store the exact ID.
- The first curated model library contains Paraformer Bilingual and Zipformer Streaming ZH. Additional models arrive with application releases, not from a remote model market.
- Settings persists a shared current model ID only. Model paths, versions and integrity are owned by model installations.
- Installation state is sourced from a colocated filesystem manifest plus required model files. SQLite is not an installation source of truth.
- A model can be current only after installation validation and runtime loading succeed.
- The current model cannot be deleted. Failed installation or loading leaves the previous selection unchanged.
- The installer validates a pinned SHA-256, prevents archive path traversal, validates required files, writes the manifest last and atomically renames a temporary directory into place.
- Network transfer is an application-layer background responsibility. The domain installer accepts an archive input and performs no network request.
- Models are stored in a Norma OS Application Support model root and are not bundled in the app or DMG.
- The domain vocabulary and Settings are recognition-engine neutral. The first runtime adapter supports Sherpa only; no dynamic plugin protocol is introduced.
- A single Sherpa recognizer factory replaces the duplicate Paraformer configuration builders.
- Paraformer uses an online Paraformer configuration requiring encoder, decoder and tokens.
- Zipformer Streaming ZH uses an online Transducer configuration requiring encoder, decoder, joiner and tokens.
- Both supported models reuse the existing 16 kHz PCM, VAD, partial/final event and dialogue client contracts.
- Voice interaction has two mutually exclusive modes: dictation and continuous listening.
- Dictation writes final text to the focused target and does not invoke Agent or TTS.
- Continuous listening sends final text into Agent/LLM/tool execution and may respond with TTS.
- `⌘⇧V` is the only voice activation entry. There is no separate master enable switch.
- Dictation stores a hold or toggle trigger preference. Continuous listening uses the same shortcut to start/stop a long session and to barge in during TTS.
- Voice Settings keeps the strongly related ASR, Agent/LLM and TTS configuration on one page, using progressive disclosure based on the current mode.
- The new persisted keys represent interaction mode, dictation trigger and immutable ASR model ID.
- Legacy Paraformer path and realtime-enabled keys are ignored and not migrated. Existing Agent/LLM credentials and provider configuration are preserved.
- The implementation is delivered in four stages: ASR domain framework, complete Paraformer vertical migration, Zipformer integration, then quality/performance release gating.
- Zipformer is not marked recommended until the fixed Chinese command set is no worse than Paraformer and target-hardware baselines are recorded.

## Testing Decisions

- Tests verify externally observable behavior through the highest practical seam and avoid assertions tied only to private helpers or source text.
- The primary seam is the existing application e2e harness: it exercises Voice Settings, interaction-mode selection, model installation states, `⌘⇧V` routing, model switching, failure recovery and persistence from the user's perspective.
- The first necessary lower seam is the public ASR domain API. It verifies immutable identities, curated lookup, manifest validation, required-file validation, archive integrity, safe extraction, atomic installation and deletion rules without network access.
- The second necessary lower seam is the Sherpa runtime factory. It verifies that an available Paraformer model selects Paraformer configuration and an available Zipformer model selects Transducer configuration with a required joiner.
- The existing dialogue ASR client tests remain the contract evidence for PCM input, partial/final text and reset behavior; they must not be duplicated in each model adapter.
- Application settings tests verify defaults, mode and trigger persistence, shared model selection, ignored legacy ASR keys and preservation of Agent/LLM configuration.
- Application dialogue tests verify that dictation bypasses Agent/TTS, continuous listening reaches Agent/TTS, and the shortcut is routed by interaction mode.
- Download orchestration tests use a local fake HTTP server and deterministic archives; unit tests do not depend on GitHub or real model downloads.
- Real Paraformer and Zipformer smoke tests are ignored tests requiring official model artifacts. Their reports explicitly distinguish domain/e2e evidence from real-app or real-microphone L3 evidence.
- The quality gate uses a small distributable Chinese command WAV/label baseline plus a team-recorded real microphone evaluation set.
- Every new Rust unit test follows the repository's complete scenario/data/execution/expected-result comment structure, includes concrete positive and negative assertions, and gives every assertion a failure explanation.
- Verification includes the ASR, dialogue, Sherpa, app and e2e suites, the application check, storage/performance boundary guards, file-length guard and no-allow guard.

## Out of Scope

- Remote dynamic model catalogs or model marketplaces.
- User-provided model URLs, arbitrary local model imports or third-party plugin installation.
- Runtime loading of untrusted provider plugins or a cross-process ASR plugin protocol.
- Non-Sherpa recognition engine implementations.
- TTS model catalog, TTS download, TTS selection or generalized ASR/TTS artifact framework extraction.
- Automatic model updates or silently repointing an existing ID to new weights.
- Bundling or redistributing Zipformer model files inside the application package.
- Migration of legacy Paraformer path or Always Listening boolean settings.
- Separate recognition model selections for dictation and continuous listening.
- Model-specific controls or conditional branches in Voice Settings.
- Declaring Zipformer recommended before Chinese quality and Apple Silicon performance evidence exists.

## Further Notes

- The exact Zipformer candidate is `sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30`, an online Chinese Transducer model compatible with the existing Sherpa Rust dependency.
- The official archive is approximately 160 MB and requires encoder, decoder, joiner and token files. UI download size must come from the pinned artifact rather than parameter-count marketing text.
- The Sherpa codebase license does not by itself establish redistribution rights for every pretrained model archive. Until the Zipformer artifact license is clarified, the product only facilitates explicit user download from the pinned official source.
- Default settings when new keys are absent are dictation mode, hold trigger and no current model. With no ready model, the shortcut shows a recoverable setup prompt instead of starting the microphone.
- This spec intentionally makes a breaking ASR/settings boundary. It preserves Agent/LLM provider data but does not preserve legacy ASR paths or the old Always Listening boolean.
