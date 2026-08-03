# Agent 任务终态 TTS 播报 SPEC

> 后续修订：其中“仅使用 Local TTS”的表述已由 `2026-07-19-SPEC-built-in-streaming-tts-providers.md` 取代；任务播报使用当前显式激活且 Ready 的 TTS Provider，失败时不跨 Provider fallback。

## Problem Statement

Norma OS 允许多个 Agent 在不同的活跃 Agent 卡片中并行执行任务。用户切换到其他卡片、最小化窗口或在其他应用中工作后，虽然卡片会显示 `Done`、`Failed` 和完成提醒，但用户仍需要反复回到画布确认每个 Agent 是否结束。这削弱了并行 Agent 的价值，也让等待任务完成变成持续占用注意力的行为。

现有本地 TTS、统一 Voice 设置、实际播放时钟和语音驱动窗口内缘光已经能够生成、播放并可视化短语音，但 Agent 任务终态尚未接入这条能力。现有 Agent hook 还把整轮任务失败与单次工具调用失败投影到相近状态；如果直接对所有 `Failed` 状态发声，会把可恢复的工具错误误报成整轮任务失败。重复 hook、UI 重绘、workspace 恢复和 PTY 退出兜底也可能造成重复播报。

用户需要一种简短、可靠、非侵入式的 Agent 任务回执：任意 Agent 卡片每轮正常完成或整轮失败时，以该 Agent 的名称播报一次；它必须与普通语音对话、Voice Preview、用户语音输入和现有窗口内缘光正确协调，并且不能把通知写入对话上下文或阻塞 GPUI 前台。

## Solution

为所有活跃 Agent 卡片增加设置可控的任务终态 TTS 播报。每次 `UserPromptSubmit` 打开一个新的 Agent 任务轮次；该轮第一次收到可信的正常完成或整轮失败证据时，应用快照 Agent 呼叫名并创建一条内存通知：正常完成朗读 `<Agent Name>, Done`，整轮失败朗读 `<Agent Name>, Failed`。没有 Agent 呼叫名的兼容场景回退到 Agent 运行时类型名称，例如 `Codex, Done`。

Agent 播报复用当前全局选择的本地 TTS 模型、Voice、音频输出和实际播放时钟，但不进入 Voice Turn，不改变 Dialogue phase，也不生成聊天气泡或对话历史。名称与终态词分别合成，并在两段 PCM 之间插入固定 250ms 静音后作为一条音频播放；该节奏只用于 Agent 任务终态播报，不改变 Dialogue、Voice Preview 或其他 TTS。播报通过有界 FIFO 队列与其他音频目的协调：普通助手回复不被 Agent 通知打断；用户开始语音输入、主动 Voice Preview 或关闭播报开关时，当前 Agent 通知立即停止，尚未播放的通知全部清空。

Agent 播报和 Voice Preview 的实际 PCM 播放复用现有语音驱动窗口内缘光。视觉仍由真实 PCM 能量与播放回调驱动，不能由 TTS 请求、合成完成或排队状态提前触发。窗口不可见时音频仍可播报，但不得为了不可见动画持续制造重绘工作。

## User Stories

1. As a user running several Agents, I want every Agent card to announce when its current task finishes, so that I can work elsewhere without repeatedly checking the canvas.
2. As a user whose Norma OS window is unfocused, I want Agent task announcements to keep playing, so that switching applications does not hide completion feedback.
3. As a user whose Norma OS window is minimized or covered, I want Agent task announcements to keep playing, so that visual occlusion does not defeat the notification.
4. As a user, I want a successful task to be announced as `<Agent Name>, Done`, so that I can immediately identify both the Agent and outcome.
5. As a user, I want an irrecoverably failed task to be announced as `<Agent Name>, Failed`, so that failure is never mistaken for success.
6. As a user, I want the announcement to use the Agent 呼叫名 shown by its active Agent card, so that the spoken identity matches the identity I use to direct work.
7. As a user of a legacy or incompletely bound Agent card, I want the runtime type name used when no Agent 呼叫名 exists, so that the announcement remains intelligible.
8. As a user, I want only the Agent name spoken rather than the task title, so that announcements stay short and do not read long or sensitive prompts aloud.
9. As a user, I want failure details omitted from speech, so that stack traces, file paths, secrets and long provider messages are not read aloud.
10. As a user, I want every task round on the same Agent card to be announced independently, so that reusing a long-lived card does not suppress later outcomes.
11. As a user, I want each task round announced at most once, so that duplicate hooks, redraws and repeated state delivery do not create repeated speech.
12. As a user, I want a recoverable tool failure not to trigger `Failed`, so that an Agent that retries and succeeds is not announced as both failed and done.
13. As a user, I want a provider-reported terminal failure to trigger `Failed`, so that confirmed task failures are audible.
14. As a user, I want an active task whose Agent process crashes or exits non-zero to trigger `Failed`, so that providers without a reliable failure hook still provide a terminal receipt.
15. As a user, I want an idle Agent process exit not to trigger `Failed`, so that closing or losing an unused session does not create a false task failure.
16. As a user, I want `SessionEnd`, manual interruption and other non-success session lifecycle events to remain silent unless they provide confirmed task-failure evidence, so that lifecycle and task outcome are not conflated.
17. As a user, I want Agent announcements to use my currently selected Local TTS model and Voice, so that all spoken product feedback has a consistent voice.
18. As a user without a configured TTS selection, I want the announcement silently skipped; when my configured selection is only checking, temporarily unavailable or already installing, I want the bounded in-memory notification retained until it becomes Ready, without an automatic download or Provider fallback.
19. As a privacy-conscious user, I want Agent announcements synthesized locally with the configured Local TTS path, so that the feature does not introduce a cloud fallback.
20. As a user, I want an `Agent task announcements` switch in Voice Settings, so that I can disable spoken task receipts without disabling ordinary voice interaction.
21. As a new or upgrading user, I want Agent task announcements enabled by default, so that the completion feature works without hidden setup once Local TTS is ready.
22. As a returning user, I want the announcement switch persisted, so that restarting Norma OS does not discard my preference.
23. As a user who disables announcements during playback, I want the current Agent announcement stopped and all pending Agent announcements cleared, so that the setting takes effect immediately.
24. As a user, I want disabling Agent announcements not to interrupt an ordinary assistant reply, so that a notification preference does not alter conversation playback.
25. As a user hearing an assistant reply, I want Agent notifications queued behind it, so that task receipts never talk over or interrupt the assistant.
26. As a user whose Agents finish close together, I want announcements played in event-arrival order, so that the audible sequence remains predictable.
27. As a user, I want no more than 32 pending Agent announcements retained, so that an extreme burst cannot create an indefinitely stale audio backlog.
28. As a user during an extreme burst, I want the oldest pending notifications dropped before newer ones, so that the remaining queue reflects the most recent task outcomes.
29. As a user beginning voice input, I want current and pending Agent announcements cancelled immediately, so that playback neither contaminates recognition nor delays my next command.
30. As a user explicitly starting Voice Preview, I want current and pending Agent announcements cancelled before preview playback, so that my direct action has priority.
31. As a user, I want an Agent announcement excluded from chat bubbles, dialogue history and LLM context, so that a system notification does not become conversational content.
32. As a user, I want Agent announcements not to show the dialogue `Replying...` state, so that a task receipt is not misrepresented as an assistant response.
33. As a user, I want Agent announcement playback to drive the existing语音驱动窗口内缘光 from actual PCM energy, so that spoken feedback has the same synchronized visual life as other TTS playback.
34. As a user previewing a Voice, I want preview playback to use the same actual-PCM window-inset animation, so that all explicitly audible Local TTS playback has consistent visual feedback.
35. As a user who disabled the window-inset visual setting, I want Agent speech to remain audible without the light effect, so that audio and visual preferences remain independent.
36. As a user, I want queued announcements to snapshot the Agent name at task termination, so that renaming a card later does not rewrite the meaning of an already-created event.
37. As a user who deletes an Agent card, I want its pending announcements removed, so that a deleted object does not speak later.
38. As a user who starts another task on the same card, I want the previous round's already queued terminal receipt preserved, so that beginning new work does not erase recent completion feedback.
39. As a user exiting Norma OS, I want pending announcements discarded rather than replayed after restart, so that an immediate notification never becomes stale startup speech.
40. As a user, I want a failed TTS synthesis or playback attempt skipped without retry, so that one broken notification cannot block or loop the queue.
41. As a user, I want later queued notifications to continue after one synthesis or playback failure, so that an isolated failure does not suppress unrelated Agent outcomes.
42. As a user, I want Agent announcements to follow the existing TTS output route and macOS system volume, so that I do not need a second volume control.
43. As a user, I want UI redraws, card focus changes, card viewing and workspace restoration to remain silent, so that only new runtime terminal evidence creates speech.
44. As a QA engineer, I want deterministic tests with fake runtime events, TTS and playback clocks, so that outcome speech and animation can be verified without network, hardware or real models.
45. As a QA engineer, I want a real macOS playback smoke test, so that headless event assertions are not mistaken for proof that audible TTS works in the packaged application.

## Implementation Decisions

- Use the domain terms **Agent 呼叫名**, **活跃 Agent 卡片**, **Agent 任务队列**, **Agent 任务回执**, **Voice Turn** and **语音驱动窗口内缘光**. “卡片名称” in the user-facing requirement resolves to the Agent 呼叫名 currently bound to the active Agent card; only compatibility fallback uses the Agent runtime type name.
- Add an application-layer Agent announcement coordinator as the single owner of task-round eligibility, terminal deduplication, name snapshots, pending announcements, audio priority and cancellation. Runtime continues to emit facts and must not synthesize speech, perform file I/O or depend on GPUI/TTS.
- Open a new announcement task round on every accepted `UserPromptSubmit` for a specific Agent node. If a trustworthy live `Stop` or `StopFailure` arrives without that start event, recover the missing round from the terminal evidence; PTY/process failure fallback still requires an already active round. Each round has an application-owned monotonically increasing identity and may claim at most one terminal announcement.
- A normal `Stop`/`Done` terminal transition claims the active round as `Done`. Duplicate `Done`, UI re-render, focus/view changes, restored snapshots and repeated delivery cannot create another notification for that round.
- A terminal provider failure such as `StopFailure` or a session-level error claims the active round as `Failed`. A tool-level failure such as `PostToolUseFailure` may update card UI or diagnostics but is non-terminal for announcement purposes and does not claim the round.
- A non-zero PTY/process exit claims `Failed` only when that Agent node has an active round and no terminal outcome has already been claimed. Idle exits, zero exits, ordinary `Ended` transitions and manual interruption do not create an announcement.
- Hook and PTY fallback evidence share the same terminal claim. Whichever valid terminal source arrives first wins; later sources for that round are ignored for speech.
- Snapshot the spoken name when the terminal outcome is accepted. Trim surrounding whitespace; if no valid Agent 呼叫名 is available, use the Agent runtime type display name. Never include the task title, prompt, tool name, provider message or failure details.
- The exact V1 utterance identities are `<Agent Name>, Done` and `<Agent Name>, Failed` for logs and observable queue state. For playback, synthesize the Agent name and terminal word separately, insert exactly 250ms of zero-value PCM between them, then submit the merged PCM as one queue item. V1 does not expose a pause setting, localize the terminal words or vary phrasing by provider.
- Add a global, immediately effective and asynchronously persisted Voice setting named `Agent task announcements`. Its default is enabled. Turning it off cancels active Agent announcement synthesis/playback, clears all pending Agent announcements and prevents new events from entering the queue.
- Treat the announcement as skipped at event time when announcements are disabled, no TTS selection is configured, credentials/Voice are invalid, or the selected local model is missing/failed. When a configured selection is only checking, temporarily unavailable, waiting for its already-started installation, or waiting for the validated runtime selection to activate, retain the terminal announcement in the existing bounded in-memory FIFO and release it once TTS becomes Ready. Do not start a model download, switch Provider, persist the queue or replay notifications skipped while disabled/unconfigured.
- Reuse the globally selected Local TTS model, Voice and validated local engine. Do not automatically download a model, invoke macOS speech synthesis, call a cloud service or silently select a different installed model.
- Introduce or extend an application-level audio-purpose coordinator so assistant reply, Voice Preview and Agent announcement cannot overlap. Agent announcements wait behind active assistant playback and play one at a time in FIFO event-arrival order.
- User-started voice input and user-started Voice Preview have priority over Agent announcements. Either confirmed action interrupts current Agent announcement work, clears its pending queue and invalidates stale async synthesis/playback completions. A raw, unclassified VAD `SpeechStart` is not yet confirmed user input and must not cancel audio by itself. Existing assistant-reply versus Voice Preview rules remain unchanged except where a prior Agent announcement must yield.
- Keep at most 32 pending Agent announcements, excluding the currently playing item. When the pending queue is full, remove the oldest pending item before appending the new item. Record overflow through bounded diagnostics without blocking the foreground path.
- Deleting an Agent card removes all of its pending announcement items. It may cancel the currently playing item for that deleted card, because the user has explicitly removed the source object. Starting a new round on the same surviving card does not remove a prior round's already queued terminal receipt.
- Keep the announcement queue and task-round delivery state in memory only. Application shutdown clears them; workspace reload must not reconstruct or replay old announcements.
- A synthesis, model-load or playback failure ends that announcement without retry, records a safe diagnostic and advances to the next pending item. Diagnostics must not log secrets or full failure payloads unnecessarily.
- Agent announcements do not enter Dialogue history, mutate Dialogue phase, display `Replying...`, generate assistant messages or become LLM context. They may reuse TTS engine and playback infrastructure without pretending to be a Voice Turn.
- Generalize the existing playback-event-to-window-inset seam by audio purpose rather than duplicating animation code. Agent announcement and Voice Preview PCM enqueue/start/finish/interrupt events drive the same语音驱动窗口内缘光 using the actual playback clock and PCM energy.
- Preserve ADR 0001: synthesis requested, synthesis completed, PCM queued and theoretical duration are not playback. The inner light begins only when audible playback starts, follows the scheduled PCM timeline, normally finishes on the playback completion callback and uses the existing interrupted fade when cancelled or failed.
- The existing window-inset visual preference remains authoritative. When it is disabled or the application window is not visible, Agent audio behavior is unchanged while visual work resolves inactive and contributes no unnecessary redraw loop.
- Playback, model loading, synthesis, cancellation waits and file/network I/O stay off the GPUI foreground path. Runtime event consumption may enqueue bounded in-memory work, but must not synthesize audio or block inside the runtime frame loop.
- The announcement feature has no independent volume. It uses the existing audio output route and system volume.
- This SPEC intentionally supersedes the existing domain statement that spoken Agent 任务回执 are limited to持续聆听. When implemented, the glossary must describe settings-controlled terminal announcements in every interaction mode and while the app is in the background.
- This SPEC also intentionally supersedes the prior Local TTS model-library rule and e2e expectation that Voice Preview never drives the voice-window inset. Preview remains isolated from Dialogue phase/history, but its actual audible PCM now participates in the shared TTS playback animation.

## Testing Decisions

- Use one primary high-level application seam: inject accepted runtime state-change/process-exit events into an app instance configured with a fake Local TTS engine and controllable playback clock, then observe emitted utterances, queue state, audio-purpose transitions, Dialogue state and window-inset visual snapshots. Prefer this seam over tests that assert private queue fields or individual helper calls.
- Test externally observable task-round behavior: a first `UserPromptSubmit` followed by `Done` produces exactly one `<name>, Done`; duplicate `Done`, redraw, focus, view and restore actions produce none; a later `UserPromptSubmit` followed by `Done` produces a second announcement.
- Test name resolution with concrete positive and negative evidence: an Agent 呼叫名 `Raya` produces `Raya, Done` and not `Codex, Done`; a missing call name produces `Codex, Done` and not an empty-name utterance; renaming after enqueue does not change the captured queued text.
- Test failure classification at the highest seam: terminal provider failure produces `<name>, Failed`; `PostToolUseFailure` alone produces no announcement; a later successful `Stop` still produces one `Done`; active non-zero PTY exit produces one `Failed`; idle or already-claimed process exit produces none.
- Test queue ordering and bounds with explicit items: assistant playback holds Agent notifications; multiple Agents then play in arrival order; 32 pending items are retained; appending item 33 drops the oldest pending item while preserving the active item and newest 32.
- Test audio priority: starting user voice input interrupts the active Agent announcement, clears pending items and causes the existing interrupted visual fade; starting Voice Preview performs the same Agent-queue cancellation before preview audio begins; an Agent notification never interrupts an assistant reply.
- Test Settings at the user interaction seam: open Voice Settings, verify `Agent task announcements` is visible and enabled by default, toggle it off, verify active/pending Agent speech stops, reload the settings store and verify the value remains off. Include positive and negative assertions that ordinary assistant playback is not cancelled by this toggle.
- Test unavailable TTS behavior: disabled announcements, no selected model, invalid installation and missing Voice each produce no queued speech, no automatic install/fallback and no later replay after configuration becomes available. Separately test that a configured selection in Checking/TemporarilyUnavailable/Installing state retains one bounded in-memory terminal notification and releases it exactly once when Ready.
- Extend the existing window-inset e2e seam with known-amplitude PCM for Agent announcement and Voice Preview purposes. Assert that queued-but-not-started audio does not light the inset, actual playback does, PCM energy changes the visual, normal completion settles, interruption fades quickly and the visual setting independently disables the effect.
- Test conversational isolation: Agent announcements leave Dialogue phase, assistant messages, history and `Replying...` unchanged before, during and after playback.
- Test Agent-only pacing with deterministic PCM: at 16,000Hz mono PCM16, derive the fixed pause as `16,000 × 250 / 1,000 × 1 channel × 2 bytes = 8,000 bytes`; assert the TTS requests are the separate name and terminal word, the middle 8,000 bytes are zero, and Dialogue/Voice Preview synthesis remains unchanged.
- Test lifecycle cleanup: deleting a card removes its pending items; beginning a new round preserves the prior queued terminal item; app shutdown/reconstruction does not replay in-memory notifications; one synthesis/playback failure advances to the next item without retry.
- Add or update headless GPUI e2e because Voice Settings, active Agent cards and the window-level visual are user-observable acceptance flows. Headless tests must be described as render-tree/application-state evidence, not proof of real speaker output.
- Add an ignored L3 macOS runner or equivalent packaged-app smoke path using a real selected Local TTS model. Verify audible `Raya, Done` and `Raya, Failed`, serial playback for two Agent cards, background-window playback, user interruption and synchronized window-inset behavior. The acceptance report must distinguish this real-app/audio evidence from headless e2e.
- Preserve existing runtime event contract tests for state-change deduplication and event-source payloads. Add runtime tests only if the runtime contract itself changes; keep announcement policy tests in the application layer where the side effect is owned.
- All new Rust tests must follow repository test requirements: complete scenario/data/execution/expected documentation, derivation comments for queue capacity/timing values, concrete positive and negative assertions, an explanatory message on every assertion, and no loops replacing individually important state assertions.
- Verification must include the directly affected crate tests, application check, e2e suite, storage/performance guardrails, line-count/no-allow checks and a real release build when the production playback path or packaged L3 runner changes.

## Out of Scope

- Reading the task title, prompt, tool name, provider error, stack trace or detailed failure reason aloud.
- Announcing single tool-call failures, permission requests, waiting-for-approval states, manual interruption, ordinary session end or idle process exit.
- Per-Agent TTS models, per-Agent Voices, per-Agent enablement, per-Agent phrasing or per-Agent volume.
- A dedicated notification volume, audio-device selector or mixer.
- macOS system speech fallback, cloud TTS fallback, automatic model download or automatic model selection.
- Persisting announcement history, restoring a pending queue after restart, replaying notifications skipped while disabled/unconfigured/invalid, or exposing a notification inbox.
- Combining a burst into summaries such as “three Agents finished,” changing FIFO to outcome priority, or retrying failed synthesis/playback.
- Writing Agent announcements into chat, Dialogue history, voice command history, Agent transcript or LLM context.
- Showing `Replying...`, creating a new Voice Turn or changing the semantic state of持续聆听/单次语音指令 for a notification.
- Adding a new animation, notification badge or card motion. This feature reuses the existing语音驱动窗口内缘光.
- Changing TTS model quality, pronunciation, language support, sample rate, buffering policy or the exact visual design/timing of the existing window inset.
- Adding a configurable announcement pause or changing global TTS speed, Dialogue pacing, Voice Preview pacing or any non-Agent-terminal TTS path.
- OS-level notifications, menu-bar status, mobile push notifications or speech while Norma OS is not running.

## Further Notes

- The first-principles value is attention release: the feature is useful because users can stop watching parallel Agent cards. Speaking only for the focused card would not solve that problem.
- A `Done`/`Failed` label currently exists in Agent card UI, but UI state is not itself a side-effect trigger. The trustworthy source is a newly accepted runtime terminal transition associated with an active task round.
- Provider capability is asymmetric. Codex versions may expose normal `Stop` without a terminal failure hook; the active-task PTY non-zero fallback is therefore required for cross-provider failure coverage, but it must share terminal deduplication with hook evidence.
- The glossary, voice-addressable Agent SPEC and prior Local TTS model-library SPEC were reconciled with these superseding rules on 2026-07-15: settings-controlled terminal announcements apply across interaction modes, and actual Voice Preview PCM participates in the shared inset without becoming Dialogue state.
- No issue-tracker publication is part of this artifact. The user explicitly requested a Markdown SPEC in the repository.
