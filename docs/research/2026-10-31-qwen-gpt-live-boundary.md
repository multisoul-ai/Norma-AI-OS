# Qwen3-ASR 旁路与 GPT-Live 的接入边界

核验日期：2026-10-31。范围：GPT-Live 官方文档与当前仓库代码。未修改业务代码，未建立收费会话，未声称旁路方案已完成或提升已实测。

## 结论

Qwen3-ASR 可以在本地重听同一份麦克风音频，帮助 Norma 后台得到更准确的任务文字。GPT-Live 继续负责自然语音对话、听说和插话。这个组合不能描述成“把 GPT-Live 内部 ASR 换成 Qwen”：Live 的音频理解仍发生在 Live 内部，应用控制的是自己的后台任务输入与可追加的上下文。[O1][O2]

建议先在旁路运行 Qwen，仅记录差异，再验证用它修订派发前的任务文本。不要在接到任意一次 Qwen 结果时立即新建任务。

## 官方协议边界

| 可核实事实 | 对方案的约束 |
|---|---|
| `session.input_transcript.delta` 有文字及 `start_ms/end_ms`，但没有完整会话轮次结束事件。[O3] | 字幕停更不等于用户说完；不能等待 API 并不存在的 complete 事件。 |
| `session.delegation.created` 只有 ID、目标与 `offset_ms`，没有完整任务文字。[O1] | 后台上下文由应用整理；该 offset 不是自动提交边界。 |
| client delegation 可将应用管理的输入直接送给后台；更正已有任务时应更新任务。[O1] | Qwen 定稿可参与后台 request，不必假装成 Live 原生 transcript。 |
| `session.thinking.append` 提供事实背景，`session.commentary.append` 提供应说出的信息，`session.instructions.append` 提供应用行为指令。[O1][O3] | 识别修正用带来源的事实背景；不能把未经核验的用户转写升级成可信系统指令。 |
| 三种 append 单次上限 500 tokens，任务更新携带既有 `delegation_id`，会话背景用 null。[O1] | 保留完整记录于应用；向 Live 追加简短修正。 |
| append ACK 确认上下文交付时序，不证明用户听到或动作完成。[O3] | 播放状态、任务执行状态、上下文接收状态分别维护。 |

官方迁移指南给出“复制来话音频给独立音频检测器”的应用架构示例。[O2] 这支持旁路方案的可行性方向；本文把检测器换成 Qwen 是工程推导，并非官方提供的 Qwen 集成。

## 当前代码已有的接缝

1. **共用音频，不再打开第二个麦克风。** `crates/app/src/app/gpt_live/capture.rs:27` 已调用 `voice_input::start_duplex_pcm_stream`。`crates/app/src/app/gpt_live/session.rs:133` 订阅 `observe_uploaded_audio()`；其观察分支已向本地 VAD 提供 PCM。这里适合新增有界的本地 ASR 分流。
2. **观察队列只能有一个消费者。** `crates/live_voice/src/connection.rs:97` 拒绝第二次订阅，容量为 64。应在当前消费者中轻量复制/转发给独立 Qwen worker，及时释放原观察对象；不要把推理塞进收发循环，避免耗尽音频预算、拖慢通话或触发容量错误。
3. **本地样本坐标与服务端时轴不同。** `crates/live_voice/src/connection.rs:43` 的 `UploadedAudio` 带 `start_sample` 与 `input_generation`；它只是本地写入观察，非服务端 ACK。`crates/live_voice/src/context.rs:101` 明确上传样本数不能单独建立服务端会话坐标。Qwen 以本地 segment ID、generation 和样本范围标记，跨源关联需要显式验证，不能用 Live offset 精确裁词。
4. **后台输入接入点明确。** `crates/app/src/app/gpt_live/dispatch.rs:125` 将 `work.snapshot.user_input` 与 conversation 交给 `NormaSession::turn_delegated`。可在生成快照前合并当前有效 Qwen 转写，而不是启动第二条独立派发链。
5. **现有更新取消与去重可复用，但需扩展来源。** `crates/app/src/app/gpt_live/bridge.rs:44` 在新用户转写使快照过期时取消旧规划；`crates/app/src/app/gpt_live/bridge.rs:88` 只认领一项工作；intent ID 以输入源锚点稳定生成。`crates/app/src/app/gpt_live/durable.rs:144` 之后使用 intent 与 operation step 持久记录副作用，不以 delegation ID 或 revision 作为第二次执行身份。

## 建议新增的数据边界

保留 Live 原始 transcript，再建立识别证据层，字段至少包括：通话 generation、本地 segment ID、音频样本范围、识别来源、转写 revision、定稿状态、对应任务 intent、取消状态。Qwen 回调只能更新仍匹配当前 generation/revision 的记录。

当前 `LiveContext::observe` 以 `event_id` 去重，并拒绝相同 event_id 对应不同内容。因此 Qwen 修订不能通过“覆盖原 Live 事件”实现，也不能把两份识别文字都追加进去，否则任务会重复或上下文冲突。应增加显式的证据选择与修订模型，生成唯一的后台有效输入。[代码：`crates/live_voice/src/context.rs:125`、`crates/live_voice/src/context/observed.rs:3`]

候选链路（方案，未实现）：

```text
同一麦克风 PCM ──→ GPT-Live ──→ 自然对话、语音回复
       │                │
       └→ 本地 VAD/Qwen  │ Live 原始转写与 delegation
              └─────────┴→ 输入证据与版本 → 唯一后台派发入口
                                              │
                                  真实执行回执 ─┴→ Live 汇报
```

## 任务执行边界

- **尚未执行：** Qwen 纠正卡片名、否定词或动作后，撤销旧候选规划，按新版本重新规划。Qwen 结果先被关联到当前输入；它的 final 仅表示该音频段转写完成，不等于用户整条请求完成。
- **已有副作用：** 不能把新文字作为新任务自动重跑。保留原 receipt，判断用户是在补充、撤销还是更换目标；取消不能自动撤回已经发出的任务。现有 durable ledger 仍是执行事实来源。
- **持续通话：** 不让 Qwen 阻塞音频上行和下行。只在需要执行任务的边界等待有效文字或澄清目标；一般闲聊保持 Live 路径。识别超时后不静默把两个冲突目标择一执行。
- **模式切换、静音与挂断：** 本地 ASR 要跟随 generation、输入开关和取消令牌；丢弃迟到结果，结束并释放音频缓存。
- **输入源竞争：** Live delegation、Qwen 定稿、未来 Jev 快速意图只能更新同一任务候选，必须指定唯一执行 owner；不能各自调用卡片派发。

应覆盖的定向用例：Qwen 先于/晚于 delegation 到达；用户后半句改目标；Live 与 Qwen 指向不同卡片；ASR 超时；挂断后迟到结果；相同输入产生多次 delegation；已发任务再出现修正；全双工外放回声被旁路误识别。先做 domain 测试与真实音频差异记录，再放开任务输入。

## 官方来源

- [O1] OpenAI Docs — Delegation and tools in GPT-Live：https://developers.openai.com/api/docs/guides/live-delegation
- [O2] OpenAI Docs — Migrate to GPT-Live：https://developers.openai.com/api/docs/guides/live-migration
- [O3] OpenAI Docs — Managing GPT-Live sessions：https://developers.openai.com/api/docs/guides/live-conversations

正文已从官方页面对应的 `.md` 端点实际获取并核对；网页与默认分支会更新。本文针对 `gpt-live-1` / Live API，不使用 Realtime 的 `conversation.item.create`、`response.cancel` 或 transcript completed 事件代替 Live 协议。
