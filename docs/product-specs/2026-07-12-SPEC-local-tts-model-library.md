# Local TTS 官方模型库 SPEC

> 2026-07-15 对齐说明：Voice Preview 的实际 PCM 现在与 Agent 终态通知共同驱动语音窗口内缘光；该行为由 [`2026-07-14-SPEC-agent-task-tts-announcements.md`](2026-07-14-SPEC-agent-task-tts-announcements.md) 取代本文早期的 Preview 视觉隔离规则。

## Problem Statement

Norma OS 的 Local TTS 目前以单一 Matcha 模型为默认实现，通过仅供测试的环境变量或固定目录加载。用户无法在 Voice Settings 中看见可用的本地语音模型，无法自主下载、验证、选择、删除模型，也无法在一个多 Voice 模型中选择和试听具体声音。

这与 Recognition Model 已具备的受控模型目录、后台下载、完整性校验、安装状态、选择和删除闭环不一致。现有回复路径还会为每次回复新建 TTS client，重复加载模型，增加首句延迟；并且仅接受 16 kHz、22.05 kHz 与 24 kHz 输出，阻碍未来接入经过验证的高采样率模型。

用户需要一个可理解、可控且离线优先的 Local TTS 模型库：自己明确决定下载什么模型，选择模型及其 Voice，并在切换、取消、损坏安装和无模型等边界下获得可预测行为。该能力必须保持 Voice Turn、Agent、TTS 播放、barge-in 与语音窗口内缘光的既有语义。

## Solution

建立与 Recognition Model 对称、但不依赖 ASR 领域的 Local TTS 官方模型库。模型库只包含随应用版本发布的精选、受验证条目；每个条目具有不可变模型 ID、固定下载来源、SHA-256、受控安装清单、后端类型、能力描述和 Voice 元数据。

用户在 Voice Settings 的 Local TTS 区块中下载、安装、选择和删除模型。模型与 Voice 分开选择：模型决定合成引擎与能力，Voice 决定该模型内具体说话人。已安装模型只占磁盘；只有当前选中的模型在后台按需加载并复用。任何模型或 Voice 配置变更都会统一取消旧 TTS 工作，并使旧异步结果失效。

首批提供一款轻量、低延迟的中文推荐模型和一款支持多个 Voice 的模型。当前 Matcha 与 Kokoro Sherpa 适配能力可作为候选，但具体模型版本、下载 URL、许可、Voice 清单、自然度、RTF 与内存基准必须在实现前实测并固定。用户不需要也不能导入任意本地模型、远程 URL 或第三方插件。

## User Stories

1. As a Norma OS voice user, I want to see Local TTS models as friendly official entries, so that I can choose a voice engine without understanding ONNX files or environment variables.
2. As a new voice user, I want no TTS model downloaded until I explicitly click Install, so that the application does not consume disk space or network bandwidth without my consent.
3. As a new voice user, I want an understandable indication that no Local TTS model is selected, so that I know why replies are text-only.
4. As a user without a selected TTS model, I want my voice command and Agent task to complete as text, so that absence of speech output does not block useful work.
5. As a user installing a model, I want visible installation progress, so that I understand the state of a potentially large download.
6. As a user installing a model, I want to cancel the download, so that I can reclaim control when network conditions or my priorities change.
7. As a user who cancels an installation, I want partial files excluded from ready models, so that a later installation cannot mistake corrupt data for a usable model.
8. As a user whose installation fails, I want a clear error and Retry action, so that I can recover without manually finding model files.
9. As a first-time user, I want my first successfully installed TTS model automatically selected with its default Voice, so that I can use spoken replies immediately.
10. As a user who already has a selected model, I want a newly installed model to remain unselected until I explicitly choose it, so that experimenting does not unexpectedly change future replies.
11. As a user, I want to select a ready model only after it has passed installation and runtime validation, so that a selected model is actually capable of synthesis.
12. As a user, I want to choose a Voice inside a compatible multi-Voice model, so that I can choose the speaking persona independently of the engine.
13. As a user selecting a single-Voice model, I want the UI to show its default Voice without irrelevant controls, so that the Settings page stays understandable.
14. As a user switching models, I want an unavailable previous Voice to fall back to the new model's recommended Voice, so that Settings never points to an invalid speaker.
15. As a user, I want to preview a Voice with a short fixed sample, so that I can compare voices before changing my assistant's replies.
16. As a user hearing an assistant reply, I want a Voice preview not to interrupt or mix into that reply, so that conversation playback remains coherent.
17. As a user changing TTS model or Voice during a reply, I want the old reply stopped immediately, so that one response never changes voice midway.
18. As a user changing TTS configuration, I want only later replies to use the new configuration, so that interrupted text is not silently replayed in a different voice.
19. As a user, I want the active selected model kept resident after its first use, so that later replies do not repeatedly pay model-loading latency.
20. As a user with several installed models, I want only the selected model resident, so that unused choices do not consume working memory.
21. As a user, I want switching model or Voice to release the old TTS engine, so that changing choices does not accumulate model memory.
22. As a user, I want deletion blocked for the selected model until I choose another model or explicitly disable Local TTS, so that Settings cannot create an accidental invalid selection.
23. As a user, I want to delete unused ready models, so that I can reclaim disk space.
24. As a user returning to the app, I want model readiness derived from on-disk validation, so that a model manually removed or corrupted outside the app is not treated as usable.
25. As a user whose selected model is missing or invalid at startup, I want replies to downgrade to text with a clear prompt, so that the app does not secretly change to a different voice.
26. As a privacy-conscious user, I want the model catalog pinned in the application release, so that the app does not fetch an unreviewed remote model list.
27. As a release owner, I want each official model tied to an immutable artifact ID, URL and hash, so that an installed selection always identifies the exact verified bytes.
28. As a release owner, I want models downloaded only after explicit user action rather than bundled in the application package, so that application size and redistribution scope remain controlled.
29. As a user of a future high-quality model, I want its verified native mono PCM16 sample rate accepted by playback, so that a 48 kHz model is not rejected by a legacy whitelist.
30. As a user, I want normal assistant replies to keep their existing barge-in, playback clock and window-inset behavior, so that adding a model selector does not regress voice interaction.
31. As a QA engineer, I want deterministic installation, selection, Voice fallback, cancellation and text-only downgrade tests, so that daily validation does not require a network or real models.
32. As a QA engineer, I want an e2e Voice Settings-to-reply seam, so that user-observable model lifecycle behavior is verified above private helper implementation.
33. As a performance engineer, I want each candidate model benchmarked for warm first audio, real-time factor, memory and cancellation, so that the official catalog excludes attractive but unusable models.
34. As a maintainer, I want TTS model identity and installation semantics outside GPUI and outside ASR, so that future Local TTS models do not create ASR-domain coupling.
35. As a maintainer, I want each asynchronous TTS operation tied to a configuration generation, so that stale downloads, loads, synthesis chunks, previews and completions can never mutate the current state.

## Implementation Decisions

- Introduce a neutral, zero-GPUI model-store domain boundary. It owns controlled roots partitioned by model kind, pinned artifact metadata, archive verification, safe extraction, temporary installation directories, manifest-last writes, atomic promotion, scanning and deletion primitives.
- Migrate reusable Recognition Model installation mechanics into the neutral model-store boundary while preserving ASR-specific capability and runtime validation in the ASR domain.
- Introduce a zero-GPUI TTS domain boundary. It owns immutable `TtsModelId`, immutable `TtsVoiceId`, curated TTS model descriptors, supported backend family, required-file validation, installation validation, capability metadata, recommended Voice and output audio contract.
- Keep model store, ASR and TTS independent of GPUI, app, storage and Sherpa. The application orchestrates network I/O and persists user preferences; the Sherpa adapter only turns validated available models into engines.
- The first catalog is compiled into each application release. It pins exact model IDs, artifact URLs, archive sizes, SHA-256 values, backend type, languages, Voice metadata and recommendation status. V1 has no dynamic remote catalog, automatic model updates or marketplace.
- The initial catalog has two product slots: a lightweight recommended Chinese model and a multi-Voice model. Candidate model versions are not promoted until license, archive layout, Chinese quality, warm latency, RTF, memory and cancellation have been measured.
- The catalog supports model-family-specific descriptors rather than treating all packages as the same file list. Matcha- and Kokoro-style loading may have different required files and Voice semantics while sharing the installation lifecycle.
- Each selected model is identified by a versioned immutable ID; each selected Voice is identified within that model. Settings never persist raw model paths, raw speaker indices or environment-variable values as product configuration.
- Persist two new Voice Settings values: selected TTS model ID and selected TTS Voice ID. Their defaults are no selected model and no selected Voice. The existing environment variables remain test-only and cannot override release Settings.
- Treat Settings as the only production source of truth for model and Voice selection. If the selected Voice is absent from a newly selected model, save that model's recommended Voice instead.
- Voice Settings presents Local TTS as a model catalog parallel to Recognition Model: uninstalled entries offer Install; installation shows progress and Cancel; failed entries offer Retry; ready unselected entries offer Select and Delete; selected entries show Selected and cannot be deleted directly.
- If no TTS model is selected, Agent and tool turns still produce text results. Spoken playback is skipped and the UI presents a recoverable setup prompt; the app does not automatically download a model.
- After successful installation, automatically select the model and its recommended Voice only when no valid TTS model was previously selected. Installing a second model leaves the existing selection unchanged.
- Downloads run off the GPUI foreground path. V1 cancellation stops transfer, discards temporary download/extraction state and leaves no ready manifest. V1 intentionally does not support resume; retry begins a fresh verified installation.
- A ready model is selectable only when file-manifest validation and a background Sherpa load validation both succeed. A failed selection preserves the prior valid selection.
- Startup scanning revalidates the selected model from the filesystem. If it is missing or invalid, it is not used and Local TTS falls back to text-only output; the app must not silently select another installed model.
- The selected TTS engine is owned by a background runtime and lazy-loaded on first synthesis or preview need. It is reused for subsequent assistant replies. Installed but unselected models remain on disk only.
- Model or Voice selection changes cancel active assistant synthesis, pending TTS chunks and assistant playback, then release the old engine. The interrupted reply is not automatically replayed. Future replies use the new selection.
- TTS runtime work is serialized or otherwise explicitly coordinated so a Voice preview cannot mutate, replace or interleave with the assistant-reply engine or playback queue.
- Distinguish audio purpose at least between assistant reply, Voice Preview and Agent task announcement. Voice Preview is cancellable and uses a fixed short sample. It does not alter dialogue phase, create assistant messages or interrupt an active assistant reply; its actual audible PCM does drive the shared voice-window inset through the same playback-clock seam.
- Every background TTS operation carries a monotonically increasing configuration generation. Model/Voice changes, disabling TTS, shutdown and relevant voice-runtime cancellation increment the generation. Completion, chunk, validation and preview events apply only if their generation remains current.
- The runtime accepts catalog-validated mono PCM16 outputs without the existing fixed three-rate whitelist. Playback configures itself from the chunk's verified sample rate. Every added sample rate requires real playback validation before catalog release.
- Existing assistant reply semantics remain intact: barge-in applies to assistant replies, playback clock continues to be the source of truth for voice-window inset, and background work never blocks GPUI.
- Existing development defaults that use Matcha paths or speaker environment variables are converted to test fixtures or explicit test factories. They are not a migration source for release-user selections.
- Existing users without persisted TTS selection start in the no-model, text-only state. No legacy inferred model path is silently adopted or downloaded.

## Testing Decisions

- The primary acceptance seam is the existing `crates/e2e` voice flow. It must exercise user-observable Voice Settings model states, installation results, model/Voice selection, a deterministic Agent reply, assistant playback cancellation and text-only fallback without asserting private implementation details.
- The e2e harness uses deterministic fake model archives, fake download transport, deterministic TTS clients and deterministic audio sinks. Daily tests must not download from GitHub, require real model artifacts, depend on microphone access or emit audible audio.
- The highest seam must cover: no model produces text-only reply and prompt; first install selects default Voice; later install remains unselected; Voice fallback on model switch; selected-model deletion protection; failed/cancelled install leaves no ready selection; corrupted selected installation falls back to text; switching during synthesis/playback prevents stale audio; Preview remains separate from assistant reply state while its actual PCM drives the shared voice-window inset.
- Model-store unit tests verify only public lifecycle behavior: immutable IDs, category-separated roots, pinned checksum verification, archive traversal rejection, required files, manifest-last order, atomic promotion, cancellation cleanup, scan results and deletion rules.
- TTS-domain unit tests verify public descriptor behavior: catalog lookup, immutable model and Voice IDs, default Voice resolution, model-specific required files, output-format declaration and invalid Voice rejection.
- Sherpa adapter tests verify that an available model selects its declared backend and Voice configuration, emits valid mono PCM16 at its declared sample rate, and propagates cancellation without further chunks.
- App-level tests verify persisted model/Voice settings, no environment override in production configuration, model/Voice changes invoking the shared TTS cancellation path, and stale-generation events being ignored.
- Existing dialogue and playback tests remain the contract evidence for reply scheduling, barge-in, speaker clock and voice-window inset. New model-management tests extend those seams rather than duplicating their state-machine assertions.
- Real downloaded-model, audible-speaker and hardware-microphone coverage remains ignored L3 smoke testing. Reports must explicitly distinguish deterministic e2e evidence from real-engine, real-audio evidence.
- A model is eligible for the official catalog only after a fixed Chinese sample suite, Voice-switch checks, cancellation checks, real speaker playback checks, warm first-playable-audio measurement, peak-memory measurement and RTF below 1.0 on the designated Apple Silicon baseline.
- All new Rust unit tests follow the repository's required scenario/data/execution/expected-result documentation format, derive numeric boundaries in comments, include explicit positive and negative assertions, and include a failure explanation on every assertion.
- Verification after implementation covers the affected domain crates, `cargo test -p e2e`, `cargo check -p app`, relevant ignored L3 tests when artifacts/hardware are available, storage/performance guards, max-file-lines guard and no-allow guard.

## Out of Scope

- Arbitrary local TTS model imports, user-entered model URLs, third-party model plugins or a generic runtime plugin protocol.
- Voice cloning, custom voice training, speaker recording, emotion controls, global speed controls and model-specific advanced generation sliders.
- Cloud TTS providers, remote streaming TTS and end-to-end speech-to-speech providers.
- Remote dynamic catalog delivery, automatic model updates, silently repointing a model ID to new weights, model marketplace browsing or background downloads.
- Download resume in V1.
- Changing ASR model selection, microphone selection, Agent provider selection, card-tool execution, the Voice Turn interaction-mode contract or the assistant playback clock contract.
- Automatically replaying an interrupted assistant reply after model or Voice changes.
- Treating test-only environment variables as supported user configuration or migrating release users from those variables.

## Further Notes

- This specification extends the Recognition Model model-management pattern while superseding the prior ASR-only decision to defer a shared ASR/TTS artifact framework. The neutral model-store extraction is intentional because Local TTS is now an approved second consumer.
- The current Local TTS implementation contains Matcha and Kokoro Sherpa loading paths, but neither should be interpreted as a release catalog guarantee. Model packaging, licensing and benchmark evidence determine the final pinned entries.
- The model library must remain a controlled supply-chain boundary: the catalog controls artifacts; model manifests control installed state; Settings controls user choice; runtime generation controls asynchronous validity.
- The canonical user behavior is: explicit install → verified ready model → explicit or first-install selection → lazy background load → reusable spoken replies. Every exception must degrade predictably to text rather than blocking the voice turn.
