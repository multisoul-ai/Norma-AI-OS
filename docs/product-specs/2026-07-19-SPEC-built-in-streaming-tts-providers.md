# 内置流式 TTS 供应商与 MiniMax 接入 SPEC

**Status:** `ready-for-agent`

## Problem Statement

Norma OS 当前只能从已安装的本地 TTS 模型和音色中选择语音合成能力。现有设计把「TTS 选择」等同于「本地模型根目录 + 模型 ID + 音色 ID」，把「可用」等同于「模型安装并校验通过」。这些假设无法表达 MiniMax 这类需要用户凭据、远程动态音色目录和网络就绪验证的 TTS 供应商，也不能在供应商之间保留独立配置。

现有 `TtsClient` 还以单个文本段为合成边界。Dialogue 每形成一个可朗读文本段，就独立调用一次合成。如果把这个边界直接映射为 MiniMax HTTP T2A，一轮长回复会重复建连并产生多个请求，既增加首音延迟和断连面，也容易消耗 MiniMax T2A 的 RPM 限额。用户需要的不是「分段请求最后各返回一段音频」，而是在模型文本继续生成时，同一轮语音合成会话持续接收文本、尽快交付 PCM，并能在插话时立即取消。

本次工作不能只做一个 MiniMax 特例。Norma OS 后续还会内置其他 TTS 供应商；如果 App、Settings、Dialogue 或播放层逐一了解每家协议，每次接入都会扩散供应商特有类型、错误、传输和配置。用户需要一个一致的 TTS 供应商选择与播放体验；维护者需要一个集中在 `tts` 能力模块内、不向 App 泄漏供应商协议的扩展方式。

## Solution

Norma OS 将 TTS 升级为用户显式选择的内置供应商能力。首期提供 On-device 和 MiniMax 两个 TTS 供应商。用户在 Voice Settings 中选择供应商、该供应商的模型和音色；当前选择统一用于助手回复、Voice Preview 和 Agent 任务终态通知。合成失败时保留文字并明确报错，不静默切换供应商。

MiniMax 首期采用 BYOK：用户输入自己的 MiniMax API Key，成功验证后密钥只存入 macOS Keychain，App 直连 MiniMax，调用费用和配额归属用户的 MiniMax 账户。首期只提供默认推荐的 `speech-2.8-turbo` 和可选的 `speech-2.8-hd`。用户验证凭据后，App 从 MiniMax `get_voice` 接口读取账户当前可用的 System、Cloned 和 Generated 音色，支持分组、搜索、选择和 Preview；Settings 同时保留可编辑 Voice ID 字段，允许用户直接填写并持久化目录之外的已知 Voice ID，但不在 Norma OS 中创建、复刻或删除音色。

合成边界由「单个文本段」升级为「一轮完整回复」。Dialogue 仍负责去除 Markdown、代码块、URL 和隐藏思考内容，并将增量模型文本提交为有序语义段。`tts` 为每轮回复建立一个可追加多个文本段、显式完成或取消、持续交付有序 PCM 块的 TTS 流。MiniMax Provider 把该流映射为一条 WebSocket 任务，在 `task_started` 后按顺序发送多个 `task_continue`，最后发送 `task_finish`。播放、PCM 缓冲、能量动画和插话行为继续复用现有供应商中立链路。

所有远程供应商实现都位于 `tts::providers`，新增厂商只增加内置 Provider 模块和注册表项，随 App 新版编译发布。不新建每厂商 crate，不支持运行时插件或任意协议 URL。`tts` 继续保持零 GPUI、零 Storage 和零 Sherpa，但允许 `providers` 内的受控网络依赖。本地 Sherpa Adapter 继续属于 `voice_sherpa`，通过同一个 TTS 流接口与 Dialogue 和播放链路组合。

## User Stories

1. As a new Norma OS user, I want to explicitly choose an On-device or MiniMax TTS provider, so that the app does not silently choose a privacy, cost, or network trade-off for me.
2. As an existing user with a working local TTS model, I want my current model and Voice preserved after upgrade, so that adding providers does not break my voice setup.
3. As a user, I want the active TTS provider shown in Spoken Replies, so that I know which system will receive and synthesize my text.
4. As a user, I want the active provider to control assistant replies, Voice Preview, and Agent terminal announcements consistently, so that Norma has one coherent spoken voice.
5. As a user, I want changing the active provider to be an explicit action, so that configuring another provider cannot unexpectedly reroute speech.
6. As a user, I want each provider to remember its own last model and Voice, so that switching back does not require repeated setup.
7. As a user, I want inactive provider profiles retained without becoming fallback candidates, so that convenience does not create hidden behavior.
8. As a user, I want a provider failure reported clearly while the complete text remains visible, so that speech failure never destroys the actual answer.
9. As a user, I want no silent fallback from MiniMax to local TTS or from local TTS to MiniMax, so that voice, privacy, and cost do not change unexpectedly.
10. As a privacy-conscious user, I want On-device TTS to remain available, so that cloud speech is optional rather than mandatory.
11. As a user who prefers cloud voice quality, I want to choose MiniMax, so that I can use its current speech models and my account Voices.
12. As a MiniMax user, I want to use my own API Key, so that charges and quotas remain attached to my own account.
13. As a security-conscious user, I want my MiniMax API Key stored only in macOS Keychain, so that it is not exposed in SQLite or ordinary diagnostics.
14. As a user replacing a saved API Key, I want the existing working credential preserved until the new key validates, so that a typo cannot destroy my working configuration.
15. As a user whose new API Key fails validation, I want the draft retained for correction without activation, so that I can fix it without re-entering everything.
16. As a user, I want a successful background connection check before MiniMax is considered available, so that field completeness is not mistaken for working speech.
17. As a user, I want validation to happen without freezing the Settings window, so that network latency never blocks the macOS UI.
18. As a returning user, I want MiniMax revalidated in the background after launch, so that expired credentials or deleted Voices are detected.
19. As a user, I want replacing credentials or manually refreshing to trigger revalidation, so that the displayed state reflects my current account.
20. As a temporarily offline user, I want my MiniMax configuration preserved while it is marked temporarily unavailable, so that a network outage does not erase setup.
21. As a user, I want Voice Setup to distinguish Configured, Checking, Ready, and Temporarily unavailable, so that I understand whether action is required.
22. As a user choosing a Voice from the verified catalog, I want Preview recommended but not required to complete configuration, so that audible hardware validation does not become a brittle setup gate.
23. As a user, I want MiniMax activation to remain disabled until the credential, supported model, and selected Voice ID are valid, so that incomplete profiles cannot become active.
24. As a user, I want clicking Use MiniMax to activate the complete profile atomically, so that my existing provider remains usable during setup.
25. As a user switching between already-ready providers, I want the change to take effect immediately, so that routine switching does not require a global Save action.
26. As a MiniMax user, I want `speech-2.8-turbo` selected by default, so that interactive dialogue prioritizes generation speed.
27. As a MiniMax user, I want to optionally choose `speech-2.8-hd`, so that I can trade additional cost and latency for higher quality and emotional rendering.
28. As a user, I want only tested MiniMax models shown, so that historical or newly announced models do not appear before Norma supports them.
29. As a user, I want the model catalog to remain stable across transient provider responses, so that my configured choice does not disappear because of an unrelated remote listing issue.
30. As a MiniMax user, I want my account's System Voices listed, so that I can choose an official Voice without copying an ID.
31. As a MiniMax user with cloned Voices, I want those Voices listed, so that assets already created in my account are usable in Norma.
32. As a MiniMax user with generated Voices, I want those Voices listed, so that generated assets already owned by my account are usable in Norma.
33. As a user, I want System, Cloned, and Generated Voices grouped distinctly, so that origin and lifecycle are understandable.
34. As a user with many Voices, I want to search the Voice catalog, so that selection does not become a long scrolling task.
35. As a user, I want to Preview a catalog Voice or a manually entered Voice ID before activation, so that I can make an informed selection and validate a directory-external ID.
36. As a user, I want a catalog-selected Voice removed from my MiniMax account detected during validation without erasing its saved Voice ID, so that Norma reports the invalid state without losing my configuration.
37. As a user, I want an editable Voice ID field retained alongside the current account Voice catalog, so that I can paste and persist a known Voice ID even when it is not listed by the catalog.
38. As a user, I want Norma not to create, clone, design, or delete MiniMax Voices in this feature, so that provider setup stays focused and predictable.
39. As a user, I want ordinary speed, volume, pitch, and automatic language handling, so that the first integration works without an overwhelming advanced panel.
40. As a user, I want 32 kHz mono PCM used consistently for MiniMax playback, so that streamed audio enters the existing playback path without a compressed-audio decoder stage.
41. As a voice user, I want audio to begin before the complete assistant reply has been synthesized, so that cloud TTS feels responsive.
42. As a voice user, I want one MiniMax stream to span one complete assistant reply, so that text segments do not create repeated network handshakes.
43. As a voice user, I want each newly committed speech segment appended to the active stream in order, so that spoken content matches the visible answer.
44. As a voice user, I want the stream finished only after the model text stream ends, so that no committed tail text is lost.
45. As a voice user, I want a new voice input to cancel the active TTS stream immediately, so that I can interrupt naturally.
46. As a voice user, I want queued but unplayed PCM discarded after cancellation, so that old speech cannot leak into the next turn.
47. As a user, I want MiniMax protocol chunks converted into the same ordered PCM type as local TTS, so that playback, energy animation, and completion callbacks remain consistent.
48. As a user, I want Markdown, code, URLs, and hidden thinking filtered identically for local and MiniMax speech, so that changing providers does not change what Norma reads aloud.
49. As a user, I want semantic speech boundaries produced consistently across providers, so that provider implementations do not reorder or reinterpret my answer.
50. As a user, I want a recoverable connection failure before MiniMax accepts the task retried once, so that a brief handshake failure does not immediately end speech.
51. As a user, I want authentication, rate-limit, and invalid-parameter errors not retried, so that known failures do not create pointless traffic.
52. As a MiniMax account owner, I want no automatic retry after `task_started`, so that an ambiguous disconnect cannot create duplicate charges.
53. As a listener, I want no replay after `task_started`, so that an ambiguous disconnect cannot repeat already spoken content.
54. As a user, I want the stream to fail once and preserve the visible response after an accepted-task disconnect, so that the outcome is honest and bounded.
55. As a user, I want Dialogue speech to retain priority over Agent notifications, so that background receipts never talk over an answer.
56. As a user, I want Agent notifications to wait for audio idle and retain their established order, so that completion feedback remains intelligible.
57. As a user, I want Voice Preview not to play simultaneously with replies or notifications, so that explicit comparison never produces overlapping voices.
58. As a user, I want a new Preview to replace only the previous Preview, so that repeated sampling remains direct and predictable.
59. As a user, I want only one active TTS stream across the app, so that providers, playback, and the window inset share one trustworthy audio timeline.
60. As a user, I want provider work and command queues kept off the GPUI foreground path, so that slow networks cannot freeze Canvas or Settings.
61. As a user, I want provider command queues bounded, so that a slow provider cannot create unbounded memory growth during a long model response.
62. As a user, I want the window inset driven by actual streamed PCM playback, so that the visual never starts merely because a WebSocket task was accepted.
63. As a user, I want normal playback completion and interruption callbacks preserved, so that streamed cloud audio has the same trustworthy end state as local audio.
64. As a maintainer, I want MiniMax protocol types contained inside the TTS capability module, so that App and Dialogue do not know `task_continue` or vendor error codes.
65. As a maintainer, I want future providers added beneath one built-in Providers module, so that provider changes remain local to the TTS capability.
66. As a maintainer, I want stable string provider IDs rather than a closed enum, so that persisted provider identities can evolve without a plugin system.
67. As a maintainer, I want all providers compiled and reviewed with the App, so that runtime code loading and unknown protocols do not become a security surface.
68. As a maintainer, I want TTS streaming interfaces owned by the TTS module rather than Dialogue, so that the capability does not depend backward on its caller.
69. As a maintainer, I want the TTS module to remain independent of GPUI, Storage, and Sherpa, so that provider protocol tests stay fast and isolated.
70. As a maintainer, I want only the Providers subtree to own remote networking, so that network behavior has clear locality.
71. As a QA engineer, I want a deterministic fake WebSocket transport, so that MiniMax event ordering and failure semantics can be tested without credentials or internet access.
72. As a QA engineer, I want tests to prove multiple text segments use one task, so that an implementation cannot regress to per-segment requests.
73. As a QA engineer, I want tests to prove PCM arrives before task completion, so that “streaming” means incremental audio rather than a streaming-shaped full response.
74. As a QA engineer, I want migration tests with a concrete existing local model and Voice, so that upgrades preserve positive state and do not invent a MiniMax selection.
75. As a QA engineer, I want Settings tested through user-visible interactions, so that credential drafts, activation, grouped Voices, and failure recovery are covered together.
76. As a QA engineer, I want a real ignored MiniMax smoke test, so that fake protocol tests are not mistaken for evidence against the live provider.
77. As a performance engineer, I want connection, task-start, first-PCM, and total-duration measurements, so that a real TTFA baseline can be established before setting an SLA.
78. As a security reviewer, I want logs free of credentials, Authorization headers, complete text, PCM, and raw custom Voice IDs, so that diagnosis does not leak sensitive content.
79. As a support engineer, I want safe MiniMax trace and session IDs retained with phase and error class, so that provider incidents can be correlated without exposing user content.
80. As a release owner, I want headless E2E, real provider smoke, and Computer Use evidence clearly distinguished, so that each acceptance claim is supported by the correct test layer.

## Implementation Decisions

- Use the established domain terms **TTS 供应商**, **TTS 选择**, **TTS 模型目录**, **TTS 音色目录**, **TTS 供应商凭据**, **远程 TTS 就绪状态**, **TTS 供应商激活**, **流式语音合成**, **TTS 流失败** 和 **TTS 输出仲裁**。不把 TTS 供应商称为 Agent Provider，不把文本分段称为音频流式输出。
- Respect ADR 0021 through ADR 0025: one streaming TTS session per reply; user-owned credentials for remote TTS; remote provider implementations inside the `tts` crate; provider-specific selections with no fallback semantics; and built-in Providers only.
- Promote `tts` from a local-model-only domain crate into the complete provider-neutral TTS capability module. It owns provider/model/Voice identities, catalogs, selections, readiness, stream lifecycle, provider registry, local model policy, and remote provider implementations.
- Change the architectural constraint from zero-network `tts` to: `tts` remains zero GPUI, zero Storage, and zero Sherpa; network dependencies and protocol code are permitted only inside the Providers responsibility. Update architecture maps and dependency guardrails to describe and enforce this new boundary.
- Keep local package identity, manifests, installation validation, and voice-reference policy grouped under the local responsibility inside `tts`. Continue to delegate Sherpa runtime construction and inference to `voice_sherpa`; `tts` must not import Sherpa.
- Add a built-in Providers registry owned by `tts`. Provider identity uses a validated stable string newtype. Initial identities are On-device and MiniMax. The registry supplies user-facing descriptors, capability metadata, model catalogs, configuration requirements, and provider construction without exposing vendor protocol types to callers.
- Do not add one crate per vendor. Future vendors are added as reviewed, compiled-in provider modules under the same `tts` capability and registered for a normal App release.
- Do not add a runtime plugin interface, dynamic library loading, arbitrary protocol URL, or user-provided provider implementation. A custom endpoint is not equivalent to a supported provider protocol.
- Move the TTS client, chunk sink, and streaming lifecycle seam from `dialogue` into `tts`. `dialogue` depends on `tts` for synthesis capability; `voice_sherpa` implements the same `tts` seam for local inference; App composes the active provider.
- Replace the single-utterance synthesis abstraction with a reply-level stream abstraction. The externally observable lifecycle supports starting one stream with an immutable selection, pushing ordered normalized text segments, finishing after all text has been submitted, cancelling promptly, and receiving ordered PCM or a typed terminal failure.
- Preserve a compatibility path only as needed during migration of current local implementations and tests. The delivered architecture must not keep per-segment synthesis as the application-level primary seam.
- Keep transport as an internal seam of each provider implementation. MiniMax production uses WebSocket; deterministic tests inject a fake transport privately. Transport injection must not enlarge the public TTS interface or leak MiniMax messages to App.
- Run all provider connection, read/write, decoding, retry, and shutdown work on background execution. Starting a provider, pushing a segment, querying Voices, or cancelling a stream must not block the GPUI foreground.
- Use bounded channels between Dialogue/App orchestration and provider workers. Preserve segment order. Queue capacity and any byte/character bound must be derived from measured segment policy and documented in tests; no provider path may use an unbounded command backlog.
- Keep `SpokenTextNormalizer` and `UtteranceCommitter` in Dialogue. They remain the single policy for visible-to-spoken normalization and semantic text boundaries. Providers receive already-normalized ordered segments and may only perform protocol framing or audio-format chunking.
- Keep the current minimum playback buffer and actual playback completion semantics unless measurement during implementation proves a provider-independent defect. This feature must not invent a separate MiniMax playback path.
- Keep the existing PCM playback type as the only output crossing the provider seam. MiniMax requests 32 kHz, mono, PCM output; provider code decodes hex to bytes, validates sample rate/channel/frame alignment, and emits valid PCM chunks. Invalid or empty audio fails safely.
- Keep energy-timeline generation, voice-window inset behavior, AVAudio playback completion, and interruption semantics downstream of the TTS seam. WebSocket acceptance or synthesis completion is not playback evidence.
- Maintain one application-wide active TTS stream. Dialogue reply speech has priority; Agent terminal notifications wait for audio idle; Voice Preview never overlaps another stream and a new Preview replaces the prior Preview; a new user voice input cancels the active stream immediately.
- Use one active MiniMax WebSocket task for one complete reply. Connect with Bearer authentication, wait for `connected_success`, send `task_start`, wait for `task_started`, send each Dialogue segment as an ordered `task_continue`, send `task_finish` after the text stream finishes, consume audio-bearing `task_continued` events, and finish only on `task_finished` or a terminal failure.
- Treat provider `base_resp.status_code` as authoritative even when the network response or WebSocket frame is otherwise syntactically valid. Preserve safe MiniMax trace and session identifiers in typed results for diagnostics.
- Retry at most once only when a retryable network connection/setup error happens before `task_started`. Authentication, rate-limit, provider parameter, schema, and cancellation outcomes are not retryable.
- After `task_started`, never automatically reconnect, recreate, or replay the task. An accepted task has ambiguous billing and audio side effects; failure terminates the speech stream, discards unplayed audio, preserves visible text, and reports a user-safe error.
- Never automatically fall back between providers. A saved inactive provider profile is configuration convenience, not a fallback list.
- Initial MiniMax model catalog contains `speech-2.8-turbo` and `speech-2.8-hd` only. Turbo is the default and recommended interactive model. The catalog is compiled with Norma and changes only after compatibility validation; it is not populated from an unverified remote model list.
- MiniMax synthesis defaults are normal speed, normal volume, zero pitch shift, no explicit emotion, automatic language boost, 32 kHz mono PCM. Do not expose provider-specific advanced controls in V1.
- Query MiniMax Voices through the authenticated `get_voice` endpoint. Normalize the response into provider-neutral Voice descriptors grouped as System, Cloned, and Generated. Preserve stable provider Voice IDs for requests while preventing raw custom IDs from entering ordinary logs.
- Support listing, searching, selecting, and previewing existing MiniMax Voices, plus direct entry of a non-empty Voice ID. Treat catalog selection and manual entry as two inputs to the same selected Voice ID rather than separate synthesis modes. Voice creation, design, cloning, deletion, upload, and lifecycle management are not part of this feature.
- Keep the selected Voice ID visible and editable in Settings. Selecting a catalog item populates the field; editing the field changes the selection to manual provenance. Persist the exact non-secret Voice ID in the provider-scoped profile. Catalog refresh, a missing catalog match, failed validation, App restart, and provider switching must not clear or rewrite it.
- Use BYOK. App receives the API Key only from Settings/Keychain composition and passes it into provider construction without persisting or logging it in `tts`. `tts` must not depend on Keychain or Storage.
- Store remote provider secrets only in macOS Keychain. Persist only non-secret provider configuration and an opaque credential reference in application settings. Credential debug output must redact secret presence and content.
- Model settings as one active provider ID plus independent provider-scoped profiles. A profile contains its model, exact Voice ID, Voice selection provenance, non-secret readiness inputs, and credential reference where required. Inactive profiles remain persisted but do not participate in synthesis.
- Migrate an existing valid local model/Voice selection to the On-device provider without changing model or Voice. Do not select MiniMax during migration. New users begin without an active TTS provider and must make an explicit Voice Setup choice.
- Separate remote provider Configured from Ready. Configured requires a credential reference, supported model, and non-empty selected Voice ID. Ready additionally requires successful background account validation and either an exact match in the latest authenticated Voice catalog or a successful explicit Preview/Validate for the current credential, model, and Voice ID tuple.
- Revalidate MiniMax on App launch, credential replacement, model or Voice ID change, and explicit refresh. A transient network failure marks the profile Temporarily unavailable while preserving all configuration. It must not erase the Keychain item, model, exact Voice ID, provenance, or prior profile. Changing any member of a manually validated credential/model/Voice tuple invalidates that validation.
- Do not require a successful Preview for a Voice ID matched by the authenticated catalog. Preview remains recommended end-to-end evidence for catalog selections. A manually entered Voice ID absent from the latest catalog requires one explicit Preview/Validate synthesis before Ready, because field syntax and account validation cannot prove that MiniMax will accept it; disclose that this validation invokes the user's provider account.
- Implement Settings as a provider-first Spoken Replies experience consistent with Apple HIG: clear hierarchy, direct selection, immediate status, no global Save, and no foreground network blocking.
- Selecting an unready provider opens a configuration draft while leaving the current active provider unchanged. API Key validation runs after the user invokes Connect. A failed draft remains editable and cannot replace a saved working credential.
- After a successful connection, show the curated model choices, searchable grouped Voice catalog, and an always-available editable Voice ID field. Preselect Turbo but require the user to select a catalog Voice or enter a Voice ID. Catalog selection fills the field; manual edits are retained verbatim after surrounding-whitespace normalization. Preview or Validate never implicitly activates the provider.
- Enable Use MiniMax only when the draft is Ready. That explicit action atomically persists the provider profile and changes the active provider. Switching between providers that are already Ready takes effect immediately.
- Invalidate any resident provider/runtime after active provider, model, Voice, or credential changes. Drop potentially blocking provider/session resources on background execution and reject stale async completions through generation/attempt identity.
- Update Voice Setup readiness so TTS readiness is provider-neutral rather than requiring an installed local model. On-device readiness continues to use installation validation; MiniMax readiness uses the remote rules above.
- Preserve all existing text/display behavior when TTS is unavailable. Voice capability follows the established product rule that selected TTS readiness is required; remote outage must surface the unavailable repair path rather than pretending pure-text voice setup is complete.
- Emit privacy-safe diagnostics for provider ID, supported model ID, provider phase, safe trace/session IDs, error class, connection latency, task-start latency, first-PCM latency, chunk count, byte count, and billed/usage character count when returned.
- Never log API Keys, Authorization headers, complete normalized or display text, PCM bytes, internal thinking, raw provider response bodies containing user content, or raw custom/cloned/generated Voice IDs. Record custom Voice category and an irreversible diagnostic fingerprint when correlation is necessary.
- Establish a real MiniMax TTFA baseline before creating a fixed release SLA. The implementation must prove incremental PCM arrives before task completion and report connection/task-start/first-PCM/total timings, but this SPEC does not invent an unsupported millisecond threshold.
- Update canonical architecture, crate maps, quality boundaries, and any older Local-TTS-only product statements when implementation lands. Reconcile prior specifications that require a selected local model or promise local-only Agent announcements with the new provider-neutral current selection.

## Testing Decisions

- The primary and highest new test seam is the public reply-level TTS stream interface in `tts`. Drive a provider selection and ordered text segments through this interface with a fake MiniMax transport, then observe only ordered PCM chunks, typed completion/failure, cancellation, and safe metadata. Avoid tests that assert private parser helpers when the same behavior is visible at this seam.
- Test the MiniMax happy-path state machine through the public stream seam: connection success, task start acknowledgement, at least two concrete text segments, ordered audio chunks, explicit finish, and terminal completion. Positively assert both segments share one transport task and negatively assert that a second WebSocket/task is not created.
- Test protocol ordering: text cannot be sent before `task_started`; `task_finish` occurs after the final segment; audio keeps arrival order; `task_finished` closes the stream exactly once; late frames after cancellation or generation invalidation are ignored.
- Test real streaming rather than response shape: deliver a first PCM chunk before the fake `task_finished`, assert the chunk reaches the sink while the stream remains active, and assert the same chunk is not re-emitted at completion.
- Test PCM validation with concrete 32 kHz mono PCM16 bytes. Cover valid even-byte frame alignment, empty audio, invalid hex, odd byte count, unsupported sample rate/channel metadata, and a provider error frame containing no audio. Every case needs explicit positive and negative assertions.
- Test multiple `task_continued` audio frames and `data: null`/missing audio events. Non-audio protocol events must not create empty playback chunks; valid later audio must still arrive.
- Test cancellation at each meaningful state: connecting, connected before task start, task started before audio, after first PCM, and while finish is pending. Assert no post-cancel PCM reaches the sink and no cancelled task is automatically recreated.
- Test retry boundaries: a retryable setup failure before `task_started` creates exactly one second attempt; a second setup failure terminates; authentication, rate-limit, and parameter errors create no retry; every failure after `task_started` creates no retry even when no PCM was received.
- Test error mapping by externally visible class and safe metadata rather than matching an entire vendor response body. Include authentication, quota/rate limit, invalid Voice, invalid model/parameter, transport, protocol/schema, audio decode, cancellation, and accepted-task disconnect.
- Test MiniMax Voice catalog normalization through the provider catalog interface. A concrete response containing one System, one Cloned, and one Generated Voice must produce exactly those three groups and must not merge or mislabel them. A missing catalog-selected Voice must make readiness fail without deleting or rewriting the saved Voice ID.
- Test manual Voice ID persistence and validation through public profile/readiness seams. A concrete directory-external ID must survive refresh, restart, failed validation, and provider switching; it must not be Ready before explicit Preview/Validate succeeds; a successful validation must apply only to the same credential, model, and Voice ID tuple; changing any member must invalidate it.
- Test credential isolation using an injected non-Keychain configuration in `tts`: debug representations and errors must not contain the secret or Authorization header. Keychain behavior itself is tested in App with the existing in-memory credential stand-in.
- Test provider registry behavior through public descriptors: On-device and MiniMax exist with stable distinct IDs; MiniMax exposes only Turbo and HD; a made-up provider ID is rejected; no runtime plugin or arbitrary URL registration exists.
- Test the On-device Adapter against the new stream seam so the architectural migration does not regress local synthesis. Push two explicit segments, assert ordered PCM and one reply-level lifecycle, and assert no network provider is selected.
- Test Dialogue at its existing streamed reply seam. Feed concrete model deltas containing Markdown, a URL, hidden thinking, and two sentence boundaries; assert the visible reply remains complete while the TTS stream receives only the expected normalized segments in order.
- Test that one Dialogue reply starts one TTS stream, pushes all committed segments, finishes after the model stream, and cancels on interruption. Negatively assert that each segment does not create its own TTS client/stream.
- Test provider-neutral playback buffering with known PCM durations. Assert playback begins only after the existing minimum buffer, a final short tail still plays, actual completion ends the reply, and provider completion without playback completion does not end the visual/audio state early.
- Test application-wide TTS output arbitration through the existing App audio-purpose seam: Dialogue prevents Agent notification playback; notifications play in arrival order after idle; Preview does not overlap and replaces a prior Preview; new voice input cancels the active stream and discards queued old PCM.
- Test bounded provider command backpressure with an explicitly derived capacity and concrete segment sizes. Assert capacity is never exceeded, ordering is preserved, GPUI work is not synchronously blocked, and cancellation releases queued commands. Do not write a magic queue size without a derivation comment.
- Test App settings persistence and migration using the settings-store seam. A concrete legacy local model and Voice must migrate to active On-device with the same IDs; MiniMax must not appear active; a new empty store must have no silent provider default.
- Test independent provider profiles: configure a concrete local selection and a MiniMax profile containing a manually entered Voice ID, switch twice, and assert each exact model/Voice pair and MiniMax provenance are restored. Negatively assert that an unavailable active provider causes automatic synthesis through the inactive profile.
- Test Keychain replacement atomically with the credential stand-in: a valid saved secret remains after a failed replacement attempt; a successful replacement changes the opaque reference/secret; removal clears readiness; debug state never contains either secret.
- Test readiness state transitions with explicit scenarios: unconfigured, configured but unchecked, checking, ready via catalog match, manual Voice ID awaiting validation, manual Voice ID validated, transient network unavailable, invalid credential, and catalog-selected Voice removed. Assert transient failure preserves fields, a manual ID remains visible, and invalid credential prevents activation.
- Test Settings at the highest headless GPUI seam available: open Voice Settings, select MiniMax, enter a test credential, invoke Connect, deliver a fake successful catalog, select Turbo and a concrete catalog Voice, assert its ID populates the editable field, Preview, and activate. Assert the previously active local provider remains active until Use MiniMax and MiniMax becomes active only after that action.
- Add a manual Voice ID Settings E2E: enter a concrete directory-external ID, assert it remains after refresh and reopen, assert activation is disabled before validation, complete a fake successful Preview/Validate, activate, switch providers twice, and assert the exact ID is restored.
- Add negative Settings E2E: failed Connect does not overwrite the saved credential or active provider; Preview does not activate; an empty Voice ID disables activation; failed manual Voice validation retains the entered ID; grouped Voice labels and search results exclude non-matching Voices; temporarily unavailable state keeps configuration visible.
- Extend the existing TTS catalog E2E rather than creating an unrelated parallel harness. Update snapshots and selectors to be provider-aware while retaining local model install/select/delete coverage.
- Add a domain-level cross-crate E2E through public APIs that combines Dialogue normalization, the `tts` fake MiniMax provider, streamed PCM, cancellation, and typed completion without GPUI. This is the highest deterministic proof of the end-to-end text-to-PCM contract.
- Add an ignored real MiniMax smoke test that reads `MINIMAX_API_KEY` from the environment without printing it. Query Voices, select an explicit safe test Voice, start Turbo WebSocket synthesis, send at least two text segments, and assert the first non-empty valid PCM arrives before `task_finished`.
- Have the real smoke report connection, task-start, first-PCM, and total duration plus safe trace/session IDs and usage counts. Treat these as baseline measurements, not a fixed pass/fail SLA beyond bounded test timeout and incremental-order proof.
- Ensure the real smoke skips with an explicit message when the environment credential is absent and never runs in default CI. Authentication/rate-limit failure with a supplied credential must fail rather than be reported as skipped.
- Add or update App/headless E2E for every user-observable acceptance flow in Settings and Voice readiness. Clearly describe it as render-tree/application-state evidence, not proof of live MiniMax or audible speaker output.
- Because this feature changes Settings layout, controls, status, and visual state, complete final UI acceptance with Computer Use against the real App. Verify provider selection, draft configuration, error feedback, grouped/searchable Voices, catalog-to-field population, manual Voice ID retention and validation, Preview, activation, switching back, and unavailable repair state.
- If packaged real-app automation is added, classify it separately from headless GPUI and real-provider protocol smoke. Do not claim that a real network smoke proves Settings behavior or that a Settings screenshot proves audible streaming.
- Preserve and extend prior art from existing Dialogue speech tests, Sherpa TTS tests, streamed reply tests, shared playback/runtime tests, Voice Settings credential tests, the TTS model catalog E2E, and the existing ignored real TTS tests.
- All new Rust tests must follow repository rules: full scenario/data/execution/expected doc comments; derivations for timeouts, sample sizes, PCM durations, queue capacities, and retry counts; explanatory messages on every assertion; concrete positive and negative assertions; no loops replacing individually important state checks.
- Verification for implementation must include tests for `tts`, `dialogue`, `voice_sherpa`, App check/tests, `e2e`, workspace-level cross-crate behavior where affected, storage and performance guardrails, max-file/no-allow/agent-guide checks, and a release App build. UI acceptance additionally requires Computer Use.

## Out of Scope

- MiniMax HTTP T2A transport, including HTTP `stream=true`; V1 production synthesis uses reply-level WebSocket only.
- MiniMax asynchronous long-text TTS.
- MiniMax Voice creation, Voice Design, Voice Cloning, audio upload, Voice deletion, or management of the seven-day activation lifecycle.
- MiniMax models outside `speech-2.8-turbo` and `speech-2.8-hd`, including 2.6, 02, and 01 series.
- Remote dynamic model discovery or automatically exposing a newly announced provider model.
- User-facing speed, volume, pitch, emotion, language boost, pronunciation dictionary, timbre mixing, subtitle, watermark, bitrate, sample-rate, format, or channel controls.
- MP3, FLAC, WAV, Opus, or G.711 playback/decoding paths for MiniMax; V1 requests PCM.
- A Norma-owned MiniMax API Key, shared desktop secret, Norma proxy, backend broker, centralized quota, subscription bundling, usage billing, or anti-abuse service.
- A user-facing cost dashboard, budget alerts, quota purchasing, or estimated currency charges. Safe usage character counts may exist only as diagnostics in V1.
- Automatic local/cloud fallback, provider priority lists, retrying another provider, or silently selecting an installed model.
- Per-Agent, per-notification, per-interaction-mode, or per-project TTS provider/model/Voice selection. The current global TTS selection remains authoritative.
- More than one simultaneous TTS stream, mixing multiple providers, overlapping speech, or a new audio mixer.
- Runtime Provider plugins, third-party dynamic code, custom provider SDK loading, arbitrary protocol URLs, or a provider marketplace.
- A separate `tts_<vendor>` crate for MiniMax or future vendors.
- Moving Sherpa inference into `tts` or allowing `tts` to depend on Sherpa.
- Changing ASR providers, Agent/LLM providers, microphone handling, VAD, voice-turn semantics, or Dialogue's visible reply content.
- Replacing Dialogue text normalization or semantic boundary policy with provider-specific text rewriting.
- Changing the visual design or timing policy of the existing voice-driven window inset beyond making streamed provider PCM participate through the existing playback seam.
- Persisting synthesized audio, replaying prior replies, offline caching of MiniMax speech, or restoring an interrupted remote stream after restart.
- Automatic retry after MiniMax accepts a task, replay after ambiguous provider state, or provider-side idempotency not documented by MiniMax.
- A fixed TTFA release SLA before real measurements establish a defensible baseline.

## Further Notes

- This specification follows the glossary changes and ADR 0021, 0022, 0023, 0024, and 0025 created during the design discussion. ADR 0017 remains authoritative for Keychain storage, and ADR 0019 remains authoritative for default diagnostic privacy.
- MiniMax official references used for the contract are the [WebSocket T2A API](https://platform.minimaxi.com/docs/api-reference/speech-t2a-websocket), [available Voice query API](https://platform.minimaxi.com/docs/api-reference/voice-management-get), [speech model guide](https://platform.minimaxi.com/docs/guides/speech-t2a-websocket), [rate limits](https://platform.minimaxi.com/docs/guides/rate-limits), and [pay-as-you-go pricing](https://platform.minimaxi.com/docs/guides/pricing-paygo).
- The essential performance decision is not merely setting a `stream` flag. It is changing the application seam from one request per committed text segment to one cancellable synthesis session per complete reply, while still allowing Dialogue to append normalized semantic segments as LLM output arrives.
- The essential extensibility decision is not a generic plugin framework. It is a deep built-in `tts` module with stable provider-neutral interfaces and private vendor protocols, so adding a provider changes one capability area and a reviewed registry rather than every caller.
- The requested publication artifact is this canonical repository Markdown SPEC. No external issue-tracker item is created by this document-generation turn.
