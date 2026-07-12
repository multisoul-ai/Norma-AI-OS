# 单次语音指令共享运行时实施计划

**Goal:** 以单次语音指令取代旧 Dictation 链路，并让它与 Continuous Listening 共用流式 ASR、Agent、工具与 TTS 回合。

**Spec:** [`docs/product-specs/2026-07-12-SPEC-single-voice-command-shared-runtime.md`](../product-specs/2026-07-12-SPEC-single-voice-command-shared-runtime.md)

**ADR:** [`docs/adr/0013-share-the-voice-turn-runtime-across-interaction-modes.md`](../adr/0013-share-the-voice-turn-runtime-across-interaction-modes.md)

## 全局约束

- 每个切片先写一个能失败的聚焦测试，再实现最小行为。
- 新增测试完整写出场景、数据构造、执行过程与预期结果；每个断言写失败说明。
- `asr`、`dialogue`、`voice_sherpa` 保持零 GPUI；模型/权限/识别与 TTS 阻塞工作不进入 GPUI 前台。
- 不使用 `#[allow(...)]`；修改后保持 Rust 源文件不超过 500 行。
- 不保留旧 Dictation 的焦点文本写入或一次性 `SpeechSession` 运行时回退路径。

## 阶段 1：先锁定新产品契约与设置迁移

1. 在 Voice Settings 单测先覆盖 `SingleVoiceCommand` 默认值、Hold/Toggle、`dictation` → `single_voice_command` 和旧 trigger key → 新 trigger key 的迁移。
2. 实现持久化读取时的兼容迁移与新值回写；旧值只作为升级输入，不再作为后续运行时分支。
3. 将 Voice 页面标签改为“单次语音指令”，将快捷键说明改为 `⌘E`，仅在单次模式显示 Hold / Toggle。
4. 将固定语音键检测、只读快捷键帮助和 e2e keystroke helper 从 `⌘⇧V` 改为 `⌘E`。
5. 先更新 e2e Settings 矩阵，验证默认值、迁移后的重启恢复、两个模式和共享模型/Provider 配置。

**Exit:** 新旧设置均能解析为单次语音指令；新持久化不再写 `dictation`；用户可在 Settings 看见正确模式和 `⌘E`。

## 阶段 2：在 Sherpa 建立无 VAD 的单次流式边界

1. 在 `voice_sherpa` 先写失败测试：给定 PCM chunk，单次 worker 发出 partial；显式结束时 flush 一个非空 Final；执行路径不要求或创建 VAD。
2. 抽取共享流式 ASR worker，使其能接收“显式结束”和“VAD SpeechEnd”两类 Final 边界，而不复制 recognizer/PCM 生命周期。
3. 让 Continuous Listening 保持现有 VAD → partial/final 行为，并以回归测试证明其没有变化。
4. 移除不再被 app 使用的一次性 `SpeechSession` / finalizer 入口及其测试，只保留与公开 API 对应的最小实现。
5. 保证 stop/cancel 会关闭 PCM、停止 worker 并 join 线程；陈旧启动结果必须可被安全释放。

**Exit:** 单次与连续路径均由同一识别模型工厂和 PCM 流启动；单次路径没有 VAD 依赖，连续路径仍具 VAD。

## 阶段 3：把 app 收敛到一个语音回合控制器

1. 先为纯快捷键决策写失败测试：单次模式的 Hold/Toggle、Thinking/TTS 中重开录音；连续模式的启动/停止、Thinking 停止和 TTS 打断回 Listening。
2. 将旧 `DictationShortcutAction`、`VoiceControlState` 和焦点文本路由替换为单次语音指令的会话策略；单次结束把 Final 送到既有 Agent 回合。
3. 让单次和连续模式使用同一个 Agent/工具/TTS 编排入口；播放结束时按模式进入 Idle 或 Listening。
4. 将 partial 事件显示到共享底栏；partial 绝不创建 pending voice turn。
5. 将模式、模型和输入设备变更接到统一 cancel：停止录音/PCM、取消 Agent 工具任务、打断 TTS、清空 pending startup；不得自动 restart。
6. 删除旧 Dictation 文案、`FinalVoiceTextAction`、Terminal 直写行为及与其绑定的测试断言。

**Exit:** app 中只有一个 ASR → Final → Agent/TTS 回合入口；两种模式只在最终边界和回复后的状态转换上分支。

## 阶段 4：补齐最高层 e2e 与回归证据

1. 将旧“Dictation 写入 Terminal”的 e2e 改为单次语音指令：Hold/Toggle 的 `⌘E` 操作各产生一轮确定性 Agent/TTS 结果，且不写入焦点 Terminal。
2. 新增 e2e：单次录音显示 partial，结束后仅有一个 Final/Agent 回合；连续模式保留 VAD 多段回合。
3. 新增 e2e：单次模式在 Thinking/TTS 按 `⌘E` 取消旧回合并开始新录音；连续模式在 TTS 仅打断并回 Listening、Thinking 则停止会话。
4. 新增 e2e：模式/模型/输入设备切换取消全部活动工作；异步启动完成后不得复活旧会话；用户再次 `⌘E` 才能启动新会话。
5. 用 deterministic ASR/LLM/TTS seam 覆盖日常测试；真实模型和麦克风 smoke 继续作为 ignored L3。

**Exit:** e2e 可观察到全部模式、快捷键、迁移、取消和回复策略，且没有旧 Dictation 直写路径。

## 验证矩阵

```bash
cargo test -p voice_sherpa
cargo test -p dialogue
cargo test -p app
cargo test -p e2e
cargo check -p app
bash scripts/check-storage-boundaries.sh
bash scripts/check-performance-guardrails.sh
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
bash scripts/check-agent-guide-lines.sh
```

真实模型或真麦克风变更时，再运行对应 ignored L3 smoke，并在验收说明中和确定性测试分开报告。
