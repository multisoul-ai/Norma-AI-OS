# 长回复 TTS 无损背压 SPEC

**Status:** `ready-for-agent`

## Problem Statement

当 LLM 生成较长回复时，用户能够在屏幕上看到完整文本，却只能听到前半部分语音。剩余文本没有进入 TTS，已经排队但尚未播放的 PCM 也可能被丢弃。这个现象在 MiniMax 上更容易出现，但根因不是 MiniMax 主动截断文本，而是 Norma OS 把正常的生产/消费速度差误判为 TTS 失败。

当前 Dialogue 将可朗读文本提交为有序 speech segment。每个 segment 最多 60 个字符；App 使用容量为 8 的 Provider command channel 和容量为 8 的前台 staging queue。MiniMax worker 对每个 segment 串行调用 `push_text`，发送一个 `task_continue` 后等待该段的 `is_final=true` 才消费下一段。当 LLM 比远程语音合成更快时，两层队列会积压；正常增量路径最多容纳约 16 个待消费 segment，而单次模型 delta 产生超过 8 个 segment 时还可能在 command channel 尚有空间前提前失败。

staging queue 超限后，当前实现调用统一的 reply failure recovery：取消当前 TTS stream、清空尚未提交的 speech segment、丢弃未播放 PCM，并使旧 generation 的后续事件失效。完整显示文本仍被保留，因此用户体验表现为“文字完整、语音只合成一部分”。

有界队列保护 GPUI 前台和进程内存的目标是正确的，但“队列暂时满”只是调度状态，不是 Provider failure。当前策略还按 segment 数量而非实际字符数衡量积压，使多个短句可能在远低于注释中的 960 字时触发失败。现有测试覆盖了 MiniMax 多帧音频、多个 `task_continue`、文本归一化、PCM 背压和取消，却没有在完整 Dialogue reply seam 上证明慢速 TTS 最终会收到一轮长回复的所有 segment。

用户需要 Norma 在 LLM 与 TTS 速度不匹配时继续显示完整回复、保持界面响应并最终朗读所有处于产品支持范围内的可朗读文本。切换到 MiniMax 不应引入静默截断；修复也不得改变 On-device、Sherpa、Kokoro、Matcha、ZipVoice、Voice Preview 或 Agent 任务回执的既有合成语义。

## Solution

Norma OS 将 Dialogue reply 的待合成文本管理收口为 Provider-neutral 的 `SpeechBacklog` domain module。它以字符预算而不是 segment 槽位管理有序积压，并区分“等待提交”“已经提交但尚未消费”“已经消费”“输入已关闭”“已取消”和“容量超限”等状态。App 继续使用非阻塞 `try_send` 将待提交 segment 填入容量为 8 的 command channel；channel 满时保留文本，等 worker 确认具体 segment 已消费后释放预算并继续填充，而不是中止 reply stream。

`TtsClient` 的 reply-level stream interface 保持不变：每轮回复仍然只调用一次 `start_stream`，按原始顺序调用多次 `push_text`，在最后一个已接受 segment 完成后调用一次 `finish_stream`，用户取消时调用 `cancel_stream`。公共层不合并、不重写、不并行提交 segment，因此所有现有 Provider 收到的文本内容、顺序和生命周期保持一致。短回复在修改前后的调用序列和 PCM 顺序必须完全相同。

首期将一轮回复在任一时刻的 active speech backlog 上限定义为 10,000 个 Unicode scalar value。active backlog 是“等待提交 + 已进入 command channel + 正在执行但尚未由 worker 确认消费”的 normalized speech text；segment 被确认消费后立即释放对应预算。因此该上限是应用的有界内存和调度政策，不是 MiniMax 的整轮累计文本限制。只要 Provider 持续消费，一轮回复的累计朗读字符可以超过 10,000，本地或远程 Provider 都不得因为累计总量达到该数值而被截断。

如果一个新 segment batch 会使 active backlog 超过 10,000，整批必须原子拒绝并产生 typed、可诊断的 `SpeechBacklogCapacityExceeded`。系统立即停止接受本轮后续 speech segment，但继续接收并显示完整 LLM 文本；此前已接受的 speech、已生成 PCM 和正在播放的音频继续按顺序完成，并在 active backlog 清空后调用一次 `finish_stream`。容量告警不得调用通用 reply failure recovery，不得清空已接受内容，也不得伪装成网络、认证或 Provider failure。Dialogue 只有在模型文本流结束、TTS terminal 完成且实际播放完成后才结束本轮生命周期。若随后发生用户取消或真实 Provider/协议/播放错误，terminal reason 优先级依次为：用户取消、真实运行错误、容量超限、成功。

TTS 网络、模型推理、PCM 解码、等待和关闭继续运行在后台 worker。GPUI 前台只执行常数级或与本次新 segment 数量线性相关的纯内存状态更新、`try_send` 和 UI 通知，不等待 channel、不访问网络、不执行文件 I/O，也不创建一组无界等待任务。现有 generation/cancellation、PCM backpressure、120ms playback prebuffer、真实播放完成和全局 TTS 输出仲裁保持不变。

本次不在公共 Dialogue 层增加 MiniMax batching 或并发 `task_continue`。如果真实 trace 表明修复无损积压后仍需提高 MiniMax 吞吐，后续优化必须留在 MiniMax Adapter 内，并通过现有 `TtsClient` interface 隐藏 Provider 特有协议。首期目标是恢复正确性，而不是改变语音韵律或 Provider 调度语义。

## User Stories

1. As a voice user, I want a long assistant reply to be spoken completely, so that I do not lose the conclusion after hearing only the beginning.
2. As a voice user, I want the spoken reply to preserve the same semantic order as the visible reply, so that audio never rearranges the answer.
3. As a MiniMax user, I want slow cloud synthesis to create backlog rather than data loss, so that network latency does not silently truncate speech.
4. As an On-device TTS user, I want the same text segmentation and synthesis calls after this change, so that fixing MiniMax does not change my local voice behavior.
5. As a user of Kokoro, Matcha, ZipVoice, or another local voice, I want short replies to sound identical after this change, so that a queue fix does not alter pronunciation or prosody.
6. As a user, I want the first eligible speech segment submitted immediately, so that long-reply correctness does not regress time to first audio.
7. As a user, I want Norma to remain responsive while TTS is slower than the LLM, so that Canvas and Settings never freeze behind speech synthesis.
8. As a user, I want temporary command-channel saturation treated as normal scheduling, so that internal pressure cannot end an otherwise healthy reply.
9. As a user, I want all accepted speech segments retained until synthesized or explicitly cancelled, so that internal queues cannot silently forget text.
10. As a user, I want `finish_stream` sent only after the final accepted segment, so that the reply tail cannot be cut off by premature completion.
11. As a user, I want short complete sentences preserved as distinct segments, so that the fix does not merge pauses or change established speech rhythm.
12. As a user, I want unpunctuated text to retain the existing 60-character forced segmentation, so that the fix does not delay first speech indefinitely.
13. As a user, I want Markdown, code blocks, URLs, and hidden thinking filtered exactly as before, so that backlog management does not change spoken content policy.
14. As a user, I want a single large LLM delta containing many sentences accepted without immediate queue failure, so that transport chunking does not affect what is spoken.
15. As a user, I want many small streaming deltas to produce the same complete result as one large delta, so that provider chunk boundaries are irrelevant.
16. As a user, I want a new confirmed voice input to cancel the current reply promptly, so that lossless backlog does not weaken natural interruption.
17. As a user, I want cancelled queued text removed, so that an old reply cannot resume after I start a new turn.
18. As a user, I want cancelled unplayed PCM discarded, so that stale audio cannot leak into the next reply.
19. As a user, I want already audible speech interrupted through the existing playback path, so that cancellation remains immediate and predictable.
20. As a user, I want the full visible reply preserved after any TTS failure, so that speech remains an output capability rather than the source of truth.
21. As a user, I want a fast-consuming TTS Provider to speak more than 10,000 cumulative characters when needed, so that a memory budget does not become a new reply-length limit.
22. As a user, I want active speech capacity exhaustion reported explicitly, so that an overloaded reply is not mistaken for successful complete speech.
23. As a user, I want accepted prefix audio preserved when active capacity is exhausted, so that overload handling does not erase valid completed work.
24. As a user, I want no automatic fallback to another TTS Provider when backlog grows, so that voice, privacy, and cost do not change unexpectedly.
25. As a user, I want Voice Preview to retain its existing isolated synthesis lifecycle, so that reply backlog changes do not affect explicit voice sampling.
26. As a user, I want Agent terminal announcements to retain their current ordering and pacing, so that Dialogue reply work does not alter notification speech.
27. As a user, I want only one active TTS output timeline across the app, so that lossless reply buffering does not create overlapping audio.
28. As a user, I want playback visuals to continue following actual PCM, so that queued text cannot light the voice inset before audio starts.
29. As a user, I want a Provider error distinguished from normal backlog pressure, so that recovery guidance reflects the real failure.
30. As a MiniMax account owner, I want the existing no-replay rule after `task_started` preserved, so that the fix cannot duplicate billed or audible content.
31. As a MiniMax user, I want one WebSocket task to continue spanning one reply, so that the fix does not add unnecessary connections.
32. As a MiniMax user, I want each `task_continue` sent in the established serial order, so that the fix does not introduce ambiguous response correlation.
33. As a local TTS user, I want model loading and resident runtime reuse unchanged, so that additional backlog does not create duplicate model instances.
34. As a user on a memory-constrained Mac, I want waiting and in-flight speech bounded by a documented active-character budget, so that correctness does not require an unbounded queue.
35. As a user, I want queue memory proportional to accepted text rather than the number of sentence boundaries, so that short sentences are not penalized disproportionately.
36. As a maintainer, I want backlog policy in one module with a small interface, so that ordering, budgets, finish, and cancellation have one source of truth.
37. As a maintainer, I want Dialogue to remain unaware of MiniMax protocol messages, so that Provider-specific changes stay inside the TTS capability.
38. As a maintainer, I want `TtsClient` unchanged, so that existing and future Provider Adapters do not require coordinated rewrites.
39. As a maintainer, I want foreground code limited to pure memory updates and non-blocking sends, so that GPUI performance constraints remain mechanically reviewable.
40. As a maintainer, I want background workers to own network and model latency, so that a slow Provider cannot block rendering or input.
41. As a maintainer, I want exact counters for accepted, submitted, consumed, and pending speech characters, so that truncation can be diagnosed without logging user text.
42. As a support engineer, I want backlog high-water marks and terminal reason logged safely, so that queue pressure can be distinguished from Provider failure.
43. As a privacy reviewer, I want diagnostics to exclude complete replies, segment contents, credentials, PCM, and raw custom Voice IDs, so that improved observability does not leak user data.
44. As a QA engineer, I want a deterministic slow TTS Adapter, so that long-reply loss can be reproduced without MiniMax credentials or internet access.
45. As a QA engineer, I want the complete Dialogue reply tested through one high-level seam, so that passing shallow queue tests cannot hide user-visible truncation.
46. As a QA engineer, I want explicit first, middle, and final segment assertions, so that a generic count cannot pass while the reply tail is wrong.
47. As a QA engineer, I want a one-delta nine-segment scenario, so that the existing premature staging failure cannot return unnoticed.
48. As a QA engineer, I want a many-delta forty-segment scenario, so that sustained producer/consumer imbalance is covered.
49. As a QA engineer, I want local TTS request and PCM order compared before and after the change, so that other Providers have concrete non-regression evidence.
50. As a QA engineer, I want cancellation tested while backlog and PCM are both pending, so that generation invalidation remains complete.
51. As a QA engineer, I want exact 10,000-active-character acceptance, 10,001-active-character rejection, and post-consumption budget reuse scenarios, so that capacity is bounded without limiting cumulative speech.
52. As a release owner, I want headless e2e and real MiniMax smoke evidence distinguished, so that deterministic orchestration proof is not confused with live Provider proof.

## Implementation Decisions

- Keep the existing domain terms **Dialogue reply stream**, **speech segment**, **TTS stream**, **TTS Provider**, **PCM playback**, **generation cancellation** and **TTS output arbitration**. Queue pressure is not named or classified as a Provider failure.
- Respect the existing reply-level TTS architecture: one logical TTS stream per assistant reply, ordered normalized speech segments, explicit finish, prompt cancellation, and ordered PCM delivery.
- Introduce `SpeechBacklog` as an in-process deep module in the pure Dialogue domain layer. Its interface owns ordered batch acceptance, active-character accounting, submission acknowledgement, input closure, cancellation, capacity results and high-water diagnostics; callers do not manipulate its internal queue. GPUI tasks, async channels, Provider clients and playback remain outside this module.
- Replace the application state's raw staging queue with `SpeechBacklog`. Keep the Provider worker command channel bounded at eight messages and retain non-blocking `try_send` from the GPUI foreground.
- Measure active backlog by normalized Unicode scalar value count, not UTF-8 bytes and not number of segments. The V1 capacity is `pending_chars + submitted_unconsumed_chars <= 10,000`. Cumulative accepted and consumed characters are diagnostic counters and do not consume capacity after acknowledgement. Every numeric test fixture must document its derivation.
- Treat command-channel full as a retryable scheduling condition internal to the reply stream. Retain the unsent segment in `SpeechBacklog`; resume filling only after a segment-consumed acknowledgement or another existing drive opportunity.
- A successful command send moves one exact segment from pending to submitted-but-unconsumed; it does not release character capacity. The worker acknowledges that segment only after `push_text` returns successfully. The acknowledgement carries enough non-content identity or character metadata to release exactly the oldest matching in-flight segment, while generation identity continues to reject stale events.
- Preserve atomic ordering when appending a batch. Either the complete batch fits the remaining active-character budget and is accepted in order, or it returns `SpeechBacklogCapacityExceeded` without partially appending the batch.
- Capacity exhaustion closes only the speech-input side of the current reply. Later model deltas still extend the visible reply but their speech output is ignored for that generation; the UtteranceCommitter must not retain an unbounded hidden tail. Already accepted text continues through submit, consume, finish and playback exactly once.
- Queue pressure and capacity exhaustion must not call generic reply failure recovery. Record and expose a user-safe capacity message when exhaustion is detected, using the existing error surface without aborting the LLM task, TTS worker or playback. User cancellation still aborts immediately. Provider, protocol, audio and playback failures continue to use their established terminal paths and take precedence over a prior capacity warning.
- Enqueue `finish_stream` exactly once when speech input is closed—because the model stream ended or active capacity was exhausted—and both pending and submitted-unconsumed segments are empty. Reply completion still waits independently for the model display stream, TTS terminal result and actual playback completion.
- Treat an unexpectedly closed command channel as a terminal worker failure unless the same generation is already cancelled. It must not leave a non-empty backlog with synthesis marked active.
- Keep `TtsClient` and `TtsChunkSink` interfaces unchanged. No Provider Adapter must learn about `SpeechBacklog`, GPUI, Dialogue state or command-channel capacity.
- Preserve exact segment content and order at the `TtsClient` seam. The common layer does not concatenate segments, change punctuation, batch small sentences, pipeline requests or add Provider-specific hints.
- Preserve the existing text policy: short-sentence merge timing, 60-character forced boundary, Markdown/code/URL normalization, hidden-thinking filtering and final-tail flush remain owned by Dialogue and are unchanged.
- Preserve one start and one terminal operation per reply: call `start_stream` once, `push_text` once per accepted segment in order, then `finish_stream` once only after pending and submitted-unconsumed speech have drained.
- Keep synthesis, network operations, PCM decoding, model inference, waits and shutdown in the existing background worker. GPUI foreground work is limited to backlog mutation, non-blocking command sends, generation checks and notifications.
- Do not spawn one waiting task per queued segment. Such tasks would form an unbounded hidden queue and complicate ordering/cancellation. Continue using the single stored reply worker task and existing generation identity.
- Preserve the existing PCM event channel capacity and its backpressure behavior. PCM channel saturation waits on the background sender and remains independent from speech-text backlog capacity.
- Preserve the existing 120ms playback prebuffer, final short-audio flush, actual playback completion callbacks, interruption behavior, PCM capture and voice-window-inset energy timeline.
- Preserve the selected resident TTS client, provider selection, model/Voice configuration and application-wide output arbitration. This SPEC adds no fallback, retry, model reload or parallel active stream. A long reply may keep the shared runtime occupied longer, so confirmed user interruption must still cancel the active generation promptly and allow later queued speech purposes to resume after cleanup.
- Limit Phase 1 changes to Dialogue reply orchestration and the new pure backlog module. Voice Preview and Agent terminal announcement synthesis do not adopt `SpeechBacklog` and retain their current direct/collected TTS paths.
- Do not modify On-device or remote Provider implementations in Phase 1. MiniMax continues to use one WebSocket task and serial `task_continue`/`is_final` boundaries. The common layer does not infer an aggregate MiniMax task limit from the per-message text limit.
- Record only the privacy-safe counters needed to diagnose this failure: cumulative accepted/submitted/consumed characters, active-character high-water mark, PCM chunk count, Provider usage characters when available, and terminal reason. Do not log text content. Additional segment-count metrics are deferred unless implementation evidence shows they are necessary.
- If a future MiniMax-only throughput change is justified by real measurements, contain it within the MiniMax Adapter behind the unchanged TTS stream interface. Updating one-task-per-reply behavior requires a separate ADR and live account verification.
- Add no SQLite migration, settings key, Keychain item, user preference, UI control or persisted queue. Backlog is reply-scoped in-memory state and is discarded on cancellation, window close or process exit.
- Update the existing built-in streaming TTS product specification or add a reconciliation note so that “bounded command queues” explicitly means bounded active memory with lossless scheduling until the documented capacity is reached, not abort-on-channel-pressure.

## Testing Decisions

- Use one primary high-level application seam: drive a complete Dialogue reply through one isolated app instance configured with a deterministic gated `TtsClient` Adapter and controllable playback, then observe submitted text, terminal order, visible reply, error state, PCM/playback outcomes and Dialogue phase. The injection must be scoped to that app/runtime instance or an equivalent isolated harness; do not rely on process-global mutable collector/gate state that can contaminate parallel tests. This is the authoritative regression seam for the user-visible bug.
- The primary regression scenario pushes forty concrete 60-character speech segments while the fake Provider consumes slowly. The data derivation is `40 × 60 = 2,400` active spoken characters, which is greater than the old `16 × 60 = 960` pending-text envelope and below the new 10,000-character active capacity. Assert the exact first, representative middle and final segments individually; assert that no bounded-queue error occurs; assert that finish follows the final segment.
- Add a second primary-seam scenario in which one model delta yields nine concrete complete sentences. Assert all nine reach the TTS stream in order and the ninth remains present; negatively assert that the old staging error is absent.
- Add a primary-seam finish scenario: hold the slow Provider with backlog pending, deliver model finish, consume commands one by one, and assert `finish_stream` is not called before the exact final segment and is called exactly once afterward.
- Add a primary-seam cancellation scenario with both unsent speech and queued PCM. Assert exact preserved/removed states: the active generation is cancelled, a named tail segment is not submitted, queued old PCM is discarded, no later old event mutates the new generation, and no finish is reported as success.
- Test `SpeechBacklog` through its own pure interface for behavior not economical at the app seam: ordered atomic batch acceptance, pending/submitted/consumed accounting, acknowledgement order, active high-water state, capacity reuse, input-closed/drained transitions, cancellation, and capacity results. Do not assert its container type or private field layout.
- The exact-capacity fixtures must derive `active = 10,000` and `rejected = 10,000 + 1 = 10,001`. Assert a concrete accepted tail exists at 10,000; assert a concrete overflow tail does not partially enter at 10,001; acknowledge a concrete 60-character segment so `active = 10,000 - 60 = 9,940`; then accept another 60-character segment and assert active returns to 10,000 while cumulative accepted becomes 10,060.
- Add a fast-consumer non-regression fixture whose cumulative speech is 12,000 characters while active speech stays below capacity. Assert one start, every concrete ordered push, one finish, no capacity result, unchanged PCM format and unchanged PCM byte order. Negatively assert that no MiniMax Provider or second client is selected. This is the key evidence that the common fix does not impose a 10,000-character reply limit on On-device, Sherpa, Kokoro, Matcha or ZipVoice.
- Preserve MiniMax fake transport tests for one task, multiple ordered `task_continue` messages, multi-frame PCM, explicit finish, accepted-task disconnect and no replay. Add long orchestration coverage at the app seam rather than duplicating App backlog concepts inside the Provider tests.
- Preserve PCM backpressure tests that fill the exact 16-event channel and prove the background sender waits until the receiver drains. Negatively assert that text-backlog changes do not turn PCM pressure into silent dropping.
- Add scheduler tests for acknowledgement and closure: a command send must keep its characters active until the matching consume event; an out-of-order or stale-generation acknowledgement must not release capacity; an unexpectedly closed command channel must terminate rather than leave Dialogue stuck in Speaking.
- Add an active-capacity terminal test. Fill exactly 10,000 active characters, reject a named next batch atomically, continue the full visible model reply, drain the exact accepted tail, call finish once, and complete playback. Assert the capacity message is visible without generic failure recovery. Then separately assert that confirmed user cancellation and a real Provider failure override the capacity terminal reason and perform their established cleanup.
- Add or update `crates/e2e` because complete long-reply speech is a user-observable Dialogue acceptance flow. Use a deterministic fake TTS transport/client; assert the complete visible reply and exact spoken first/final content. Describe this as headless application-state/playback evidence, not proof of real speaker output.
- Keep the existing ignored real MiniMax smoke test as optional L3 evidence. When credentials are explicitly available, add a representative multi-segment fixture and compare requested character totals with returned usage characters without printing API Key, complete text or raw Voice ID.
- Do not add Computer Use acceptance for backlog scheduling alone. If the capacity path renders a new user-facing message through the existing error surface, use Computer Use on the real App to verify that visible state and its recovery, even though no layout redesign is planned.
- Run directly affected verification: Dialogue tests, TTS tests, voice_sherpa tests, App check/tests, e2e suite, TTS Provider boundary guard, performance guardrails, maximum-file-line check and no-allow check. Run workspace tests when shared TTS contracts or cross-crate behavior change.
- All new Rust tests must follow repository requirements: complete scenario/data/execution/expected documentation, derivation comments for capacities and character budgets, concrete positive and negative assertions, an explanatory message on every assertion, and no loop replacing individually important state assertions.

## Out of Scope

- Changing the `TtsClient` or `TtsChunkSink` interface.
- Changing On-device, Sherpa, Kokoro, Matcha, ZipVoice or other Provider synthesis implementations in Phase 1.
- Changing speech normalization, thinking/code filtering, short-sentence merge timing or the 60-character forced segment policy.
- Adding a Provider-neutral limit on cumulative spoken characters per reply. The 10,000-character capacity applies only to currently active, unconsumed speech and is reusable after acknowledgement.
- Common-layer sentence batching, text rewriting, prosody changes or provider-specific branching in Dialogue.
- Pipelining multiple MiniMax `task_continue` messages before receiving per-segment `is_final`.
- Automatically opening a second MiniMax WebSocket task based on cumulative reply length.
- Changing MiniMax model, Voice, PCM format, sample rate, retry, billing, authentication or accepted-task replay semantics.
- Changing playback prebuffer duration, PCM event capacity, audio device selection, volume, energy animation or playback completion semantics.
- Changing Voice Preview, Agent task announcement wording/pacing/queueing or TTS output priority.
- Adding unbounded channels, blocking GPUI sends, one waiting async task per segment, disk spooling or SQLite-backed TTS queues.
- Adding Settings controls for backlog size, speech length or Provider throughput.
- Persisting backlog across reply cancellation, window close, workspace reload or application restart.
- Silent fallback to another Provider when backlog, length, network, quota or synthesis errors occur.
- Establishing a new MiniMax latency SLA or optimizing TTFA before live measurement.

## Further Notes

- This SPEC refines the bounded-queue requirement in the built-in streaming TTS specification. Bounded active memory remains mandatory; aborting a healthy reply merely because the command channel is temporarily full is not an acceptable interpretation of that requirement.
- The current full display reply is already retained independently of the speech queues. The old eight-segment staging limit therefore provides little protection against total reply memory while creating a much earlier data-loss threshold. A reusable active-character budget better matches memory risk without imposing a cumulative reply-length cap on fast Providers.
- MiniMax latency is a reproduction amplifier, not the direct truncation decision. Its public WebSocket contract supports multiple ordered `task_continue` events, and each current Dialogue segment remains far below the documented per-message text limit. The App currently chooses to serialize per-segment submission and to abort on local staging pressure; this SPEC does not claim that MiniMax imposes a 10,000-character aggregate task limit.
- The first-principles requirement is correctness under producer/consumer mismatch: accepted speech work must be completed, explicitly cancelled or explicitly rejected at the documented active-capacity boundary. Internal channel pressure cannot silently redefine success.
- The `SpeechBacklog` design follows a deep-module seam: callers submit ordered speech and observe submit/acknowledge/drain/cancel/capacity results, while active-character accounting and state invariants remain local. GPUI concurrency guidance keeps foreground work non-blocking and retains Provider latency in the stored background worker.
- No issue-tracker publication is part of this artifact. The user explicitly requested a Markdown SPEC in the repository.
