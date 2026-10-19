---
title: Norma × GPT-Live 实时双工接入方案
subtitle: 参考桌面豆包实现，接入真正的持续听说与后台 Agent 委派
template: doc
theme: paper
lang: zh
date: 2026-10-18
status: 调研与建议 · 尚未实现或云端验收
---

建议新增独立的 GPT-Live 通话模式：`gpt-live-1` 负责持续听说，现有 Norma Agent 负责推理和项目工具。首个桌面 POC 用 WebSocket 验证复用路径；以真实声学和网络结果决定正式媒体传输。

## A 接入什么，先做什么

```callout ok 核心选择
采用 GPT-Live + client delegation。持续对话留在 Live，项目查询、派工、记忆和取消交给 Norma。后台任务执行期间，用户仍可补充条件、纠正和交谈。
```

| 决策 | 推荐 | 原因与边界 |
| --- | --- | --- |
| 目标产品 | `gpt-live-1` / Live API | 官方支持同时听说；有独立协议。 |
| 后台委派 | `delegation.type = client` | 保留 Norma 的模型选择、工具、权限和任务记录。 |
| 首轮范围 | macOS 桌面 | 参照当前桌面豆包；iOS 后续独立适配。 |
| 技术 POC | WS + 16 kHz PCM16 | 直接接现有采集与播放能力，减少原生媒体接入变量。 |
| 正式传输 | 以 WebRTC 为官方推荐基线评估 | WS 是可用选项；需承担缓冲、节奏、抖动和播放管理。 |
| 普通闲聊 | Live 直接回答 | 仅需要后台事实或执行时委派，避免重复提交同一请求。 |

这是对现有工程的接入建议。OpenAI 官方对浏览器、移动端和服务端音频均推荐 WebRTC。WS POC 通过不能证明弱网体验已达标。[入门](https://developers.openai.com/api/docs/guides/live) · [连接选择](https://developers.openai.com/api/docs/guides/voice-websockets?api=live) · [委派模式](https://developers.openai.com/api/docs/guides/live-delegation)

## B 豆包现状能复用哪些能力

**源码现状：桌面豆包使用 ASR → Chat → TTS。** 持续模式保持收音，检测到说话后取消旧回复并停止播放。它与早期 Seeduplex 原生双工规格存在差异。

```flow LR
麦克风 -> 豆包ASR: 16k PCM
豆包ASR -> 豆包Chat: 最终转写
豆包Chat -> Norma工具: 函数调用
Norma工具 -> 豆包Chat: 真实回执
豆包Chat -> 豆包TTS: 回复文本
豆包TTS -> 播放器: 24k PCM
```

| 当前事实 | 源码证据 | 接入意义 |
| --- | --- | --- |
| 独立 ASR、Chat、TTS 编排 | `crates/app/src/app/doubao/plan.rs:47` | 参考生命周期和边界；新增 Live 协议适配。 |
| ASR Plan WebSocket | `crates/app/src/app/doubao/plan/asr.rs:66` | 豆包二进制帧不能用于 Live 的 JSON/base64 协议。 |
| Chat 使用 `doubao-seed-2.0-mini` | `crates/app/src/app/doubao/plan/chat.rs:17`、`:187` | 当前不是 Seeduplex 原生语音模型。 |
| 收音为 16 kHz、可启用系统 AEC | `crates/voice_input/src/audio_capture.rs:10`、`:138`；`stream.rs:106` | 复用回声消除和设备能力；真实外放仍要验收。 |
| 播放队列有 epoch 和完成回调 | `crates/app/src/app/doubao/playback.rs:30` | 可抽取共享播放器；需适配 Live 连续流语义。 |
| 工具经持久化账本去重 | `crates/app/src/app/doubao/tools.rs:76` | 复用 reservation、unknown 和回执处理规则。 |
| 工具 Adapter 校验项目和 generation | `crates/app/src/app/agent_runtime/doubao_adapter.rs:29` | 保留切项目、挂断后拒绝迟到调用的边界。 |
| 模式只有 Legacy / Doubao | `crates/app/src/settings/voice/state/mode.rs:2` | 新增 GptLive；不改变旧配置默认值。 |

现有真实豆包测试也明确验证三个 Plan 服务，且不依赖 Seeduplex。证据：`crates/app/tests/doubao_real.rs:23`。历史调研和 SPEC 可解释设计意图，当前行为以源码为准。

## C 新增结构怎样接入现有 Agent

```flow LR
麦克风 -> AEC采集: 现有能力
+ AEC采集 -> Live会话: 连续输入
+ Live会话 -> 播放器: 连续输出
播放器 -> 扬声器
+ Live会话 -> 上下文桥: 转写与委派
+ 上下文桥 -> NormaSession: 后台任务
NormaSession -> NormaToolRuntime
NormaToolRuntime -> 桌面Adapter
桌面Adapter -> 项目工具
+ NormaSession -> 上下文桥: 已核验结果
+ 上下文桥 -> Live会话: 简短事实
```

图中新增路径为建议结构，尚未实现。项目工具继续使用既有执行路径。Live 前端不维护第二套 Canvas、Terminal 或 AgentCard 操作。

```tree
+ crates/live_voice/ | 新增，零 GPUI / Storage 的 Live 能力层
  + src/session.rs | 连接生命周期与有界收发
  + src/protocol.rs | 官方事件与配置
  + src/context.rs | 双向转写时间线与委派关联
  + src/providers/openai.rs | Live WS 连接与协议
+ crates/app/src/app/gpt_live/ | 采集、播放、后台 Agent 与 UI 编排
+ crates/app/src/app/voice/playback_queue.rs | 从豆包抽取实际共享的播放能力
~ crates/agent/src/master.rs | 接收外部委派上下文和操作关联
~ crates/agent/src/model.rs | 增加明确的 Live 输入来源
~ crates/app/src/settings/voice/ | 模式、凭据、音色与就绪状态
```

新增 crate 只接收音频数据与事件，不访问麦克风、AppDB 或桌面 UI。依赖建议为 `live_voice → audio`，由 `app` 注入配置与音频。不要注册成 ASR/TTS Provider，也不要塞进旧 `DialoguePhase`。

`NormaSession::turn_streaming` 已支持输入来源；当前 `ToolContext.operation_id` 默认为空。接入前需新增显式委派上下文入口，将关联信息传到工具边界。持久化 reservation 仍在 app 后台完成，不能进入 agent 域层。

为通话创建独立的 NormaSession，复用实现与配置，隔离协议历史和结果路由。后台结果只回传 Live，避免再次触发旧语音链路的 TTS。

证据：`crates/agent/src/master.rs:177`、`:208`。现有 `DoubaoVoice` 还触发供应商专属分支，Live 不应伪装成这个来源。需逐项核对授权和删除规则，使用专属来源及明确能力。证据：`crates/agent/src/model.rs:6`、`adapter.rs:268`；`crates/app/src/app/agent_runtime/adapter_pump.rs:132`。

## D 官方接入问答：协议需要怎样写

| 问题 | 已核实的官方答案 |
| --- | --- |
| 地址和模型？ | WS：`wss://api.openai.com/v1/live/sessions`，无查询参数。模型放在启动配置中。 |
| 如何鉴权？ | 服务端用 `Authorization: Bearer …`。项目 Key 留在可信服务端。 |
| 首包是什么？ | `session.start`；收到 `session.started` 后再发音频和应用命令。 |
| 音频怎样发？ | JSON 文本消息，`session.input_audio.append.audio` 为原始音频 base64。 |
| 支持现有 16k 吗？ | 支持 16k / 24k、单声道 PCM16LE。启动时选择一个上下行共同格式。 |
| 有哪些收包？ | `session.output_audio.delta`；用户和助手各自的 transcript delta；委派、错误和用量事件。 |
| 何时提交一轮？ | 连续送入真实节奏的音频，包括静音。Live 管理听说时机，无需逐句 commit。 |
| 如何结束？ | 先监听 `session.closed`，发送 `session.close`，等最终事件后释放连接。 |

依据：[Primary WebSocket API Reference](https://developers.openai.com/api/reference/resources/live/primary-websocket) · [WebSocket 指南](https://developers.openai.com/api/docs/guides/voice-websockets?api=live)。

首版启动配置草案：

```json title="Live WS 启动配置 · 草案"
{
  "type": "session.start",
  "event_id": "start_1",
  "session": {
    "model": "gpt-live-1",
    "instructions": "你是 Norma。用简短中文自然交谈。项目查询、操作、记忆及修正交给后台。只依据真实回执报告结果。",
    "audio": {
      "format": { "type": "audio/pcm", "rate": 16000 },
      "output": { "voice": "marin" }
    },
    "delegation": { "type": "client" },
    "store": false
  }
}
```

实际 instructions 从 Norma 人设生成。语气、语言和插话规则放在 Live；详细工具 schema、权限和任务工作流留在后台。[提示词指南](https://developers.openai.com/api/docs/guides/live-prompting) · [迁移指南](https://developers.openai.com/api/docs/guides/live-migration)

```callout warn 与旧 Realtime 协议分开
GPT-Live 不使用 /v1/realtime?model=…，也不沿用 input_audio_buffer.commit、response.audio.done 或逐轮 response.cancel。client delegation 的结果用 append 事件返回。response.create 在 Live 中用于 Responses 后台工作，不是启动每次语音回答。
```

WebRTC 则通过 `POST /v1/live/sessions` 交换 SDP。HTTP 创建已启动会话，数据通道不再发 `session.start`。如需 sideband，用 `session_id` 连接 `/v1/live/sessions/{session_id}/attach`。现有手机 Realtime 的 `call_id` 链路需要独立适配。[WebRTC](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live) · [服务端控制](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live)

## E 用户边说边改，任务如何可靠执行

```sequence num
participants: 用户, Live, 上下文桥, Norma后台, 项目工具
用户 -> Live: 让 Simon 检查当前项目
Live -> 上下文桥: 转写片段和 delegation.id
上下文桥 -> Norma后台: 会话上下文与请求版本
Norma后台 -> 项目工具: 查询或执行已授权操作
用户 -> Live: 先只看登录模块
Live -> 上下文桥: 新转写与委派通知
上下文桥 -> Norma后台: 修正请求，更新版本
项目工具 --> Norma后台: 真实结果与 task_id
Norma后台 --> 上下文桥: 当前版本的核验结果
上下文桥 -> Live: commentary.append
Live --> 用户: 播报进展或结果
```

这是应用处理示意，不保证修正发生时旧操作尚未执行。写操作前必须核对当前请求版本；已产生副作用时如实报告，再走现有修正或取消工具。

**关键事实：`session.delegation.created` 没有任务正文。** 它提供 `delegation.id`、`target` 和 `offset_ms`。桥接层需保存双向 transcript 片段、时间和当前项目状态。不能把每个片段直接变成一次 Agent 请求，也不能把短暂收包空隙当作一句结束。[委派上下文](https://developers.openai.com/api/docs/guides/live-delegation#keep-the-conversation-context-in-your-application)

| 桥接规则 | 具体处理 |
| --- | --- |
| 上下文充分才派工 | 保存原始片段；结合委派时间、当前请求和近期双向对话形成快照。信息不足时挂起并澄清。 |
| 跨事件乱序 | 委派可能先于相关转写抵达；保留通知并补齐上下文，不能拿空文本执行。 |
| 修正优先 | 维护 `request_revision`；执行前和结果回传前都核对版本。 |
| ID 分层 | `session_id`、`delegation_id`、后台请求 ID、`tool_call_id`、`task_id` 分别记录。 |
| 持久化去重 | 对委派和工具分别登记；工具键绑定项目、会话、委派、修订及工具调用。重复通知不再执行。 |
| 不宣称 exactly-once | 崩溃窗口、跨新委派重复意图需查账和核对；unknown 不自动重试写操作。 |
| 取消分层 | 插话打断语音；显式取消才进入任务取消路径。新派工仍须校验当前项目。 |

未提交的后台推理可在修正后取消重算。已接收的项目任务保持其真实生命周期，不能用 `CancelLatest` 混同“停止说话”和“取消任务”。

后台进展用 `session.thinking.append`；可播事实用 `session.commentary.append`。两者都填写原 `delegation_id`，通用会话上下文用 `null`。单次 content 上限 500 tokens，返回简短完整事实。[结果回传](https://developers.openai.com/api/docs/guides/live-delegation#send-the-right-kind-of-update)

不要直接播报后台 token delta。只转发可独立理解、经核实的进展和最终事实。工具“已接收”与 Agent 任务“已完成”保持不同状态。

## F 怎样保证是真双工

**输入、输出和后台工作独立推进。** 模型说话时继续上传麦克风；后台执行时持续处理音频、字幕和新的委派。

| 维度 | 建议状态与行为 |
| --- | --- |
| 连接 | Idle / Connecting / Active / Closing / Failed |
| 输入 | Capture / Muted；不随输出播放停止 |
| 输出 | Playing / Quiet / LocallyBlocked；以播放器和音频活动为依据 |
| 后台 | 多条 delegation / task 的独立状态；同一 NormaSession 的 turn 串行 |
| 字幕 | 用户与助手各自增长；保留 `start_ms` / `end_ms`，允许重叠与迟到 |
| 本地停止播报 | 停 sink、清队列、更新播放 epoch；继续接收事件和输入音频 |
| 切项目或模式 | 更新会话 generation，关闭旧连接；迟到音频和新工具请求失效 |

音频发送必须有界并按采样节奏推进。现有 AEC 输入队列最多保留 8 个 chunk；需要记录拥塞和丢弃量。输出队列按音频时长设预算，不能只按包数限制，也不能等待网络音频收完才播放。

**自然附和不必每次硬停。** 默认让 Live 处理听说和自然插话。明确停止操作可本地清空输出，并发送应用指令纠正模型；恢复播放需有应用规则。单凭指令 ACK 不能证明旧音频已清空。[播放控制](https://developers.openai.com/api/docs/guides/voice-server-controls?api=live#control-playback-when-needed)

```callout warn 连续流没有单句播放终点
主 WS 的 output_audio.delta 没有音频时间戳，也没有 output-audio-done。transcript 时间只表示近似片段时段。commentary.appended 不等于用户已听完。
```

因此需拆开“后台结果已发送”“模型已生成字幕”“本地音频已播放”。可复用硬件播放回调观察已消费的 PCM；短暂静音或输出 VAD 只用于活动指示，不能证明整条汇报完整。首版对任务播报保留“待核对”，不沿用豆包 TTS `finish_for` 的单回复完成判定。[字幕与播放](https://developers.openai.com/api/docs/guides/live-conversations#manage-speech-and-transcripts)

扬声器播放与 AEC 参考路径必须在真实 Mac 上验证。验收包括耳机、内置外放、音量变化、说话重叠和重新开麦；“启用了系统 AEC”不等于声学验收通过。

## G 配置、人设、凭据和成本

| 用户看到的内容 | 推荐行为 |
| --- | --- |
| Settings → Voice | 增加 GPT-Live，与 Voice Commands、Doubao 并列。 |
| 核心配置 | 凭据连接、音色、麦克风；后台沿用现有 Norma AI 配置。 |
| 就绪状态 | 通话就绪与项目工具就绪分别显示；缺后台配置不阻止普通聊天。 |
| 通话状态 | 明确显示连接、麦克风、输出播放和后台任务；可同时听说。 |
| 操作 | 沿用可配置语音快捷键，提供静音、结束通话和明确的任务取消入口。 |
| 错误 | 分开提示凭据无效、模型无权限、网络失败、音频失败、后台不可用。 |

设计采用 `apple-design` 的信息层级、即时反馈和可访问性。模式选择沿用仓库 Material 菜单规范。避免用互斥的“听 / 想 / 说”遮住并发状态；按下与焦点即时可见。深浅主题和键盘操作用真实 App 验收；本次 HTML 不添加装饰动效。

凭据部署有两个场景：

- 个人桌面 POC：用户自带 Key，沿用 AppDB 凭据引用机制，在本机后台连接。这是产品部署建议，不是官方服务端安全模型的同义表述。
- 面向用户分发统一项目 Key：使用可信服务端持有 Key，提供会话创建或媒体代理。不能把共享 Key 打包进 App 或 HTML。WebRTC 的 SDP 交换仍由可信服务端完成。

官方要求项目密钥保留在可信服务端。此次没有读取本机凭据，也没有验证目标账号权限。[连接与认证](https://developers.openai.com/api/docs/guides/voice-webrtc?api=live)

人设从现有 Persona 派生；近期话题与已核验事实用 context append 更新。完整工具规则、长期记忆和项目状态由后台持有。项目切换后建立新通话，重新取上下文。[迁移指引](https://developers.openai.com/api/docs/guides/live-migration)

| 费用与数据 | 官方事实 / 方案边界 |
| --- | --- |
| 语音费用 | 当前公开价 $0.05 / 分钟，按秒计费；1 小时约 $3，另加后台模型与工具。 |
| 静音和等待 | 活跃会话的静音、双方未说话及后台等待也计时。静音不结束计费。 |
| WebRTC 初始化 | 创建请求计 15 秒语音时长，启动后抵扣运行时长；不能再额外叠加 15 秒。 |
| 用量 | `session.usage.updated` 是累计快照，取最新值；最终以 `session.closed` 为准。 |
| 并发 | 模型页列 Free 不支持，Build 50 / Launch 300 / Grow 500；目标项目以实际权限为准。 |
| 首版保存 | 显式 `store: false`，本机不保存原始音频，不自动合并旧聊天历史。 |
| 云端保留 | store=false 不等于 ZDR；默认滥用监控保留与后台端点规则仍需适用。 |

依据：[模型与价格](https://developers.openai.com/api/docs/models/gpt-live-1) · [计费说明](https://developers.openai.com/api/docs/guides/voice-latency-cost?api=live) · [数据控制](https://developers.openai.com/api/docs/guides/your-data#v1livesessions)。

## H 实施顺序与验收证据

```timeline v
P0 | 协议与上下文 POC | 假上游、16k 连续音频、双向转写、委派乱序、正常关闭。
P1 | 现有 Agent 闭环 | 请求修订、专属来源、持久化去重、真实 task_id 和独立取消。
P2 | 真实通话与产品化 | 耳机与外放、长对话、弱网、设置入口、深浅主题与费用观察。
```

P0/P1 先写可失败的聚焦测试再实现。P2 的媒体传输选择以实测为门槛：WS 若在目标网络出现不可接受的延迟和积压，就进入 WebRTC 原生接入；不要靠扩大队列掩盖问题。

| 验收标准 | 对应证据计划 |
| --- | --- |
| 持续真双工 | 假上游在输出 PCM 时观察输入仍到达；真实 App 验证同时听说。 |
| 委派不丢上下文 | `live_voice` 单测覆盖通知先到、迟到字幕、“可以”、同轮纠正和无上下文。 |
| 项目操作不重复 | app + agent 集成测试覆盖重复 ID、双委派同意图、断线重建、账本 unknown。 |
| 授权不变宽 | agent 测试覆盖讨论语气、删除目标、无项目、旧项目、错误输入来源。 |
| 插话不误取消任务 | 真实工具测试：打断播放后 task_id 继续运行；显式取消走真实取消路径。 |
| 结果状态真实 | accepted 不报完成；旧 revision 结果不播；没有播放终点时不标“已完整播报”。 |
| 资源全部收口 | 测试关闭超时、迟到包、切模式和重开；麦克风及播放器在后台释放。 |
| 网络和代理可用 | 测目标代理、鉴权失败、慢连接、上下行拥塞和断线；不重放未知写操作。 |
| 用户配置可用 | 新增 `test/desktop-e2e/Scenarios/voice/` 模块，检查模式保存、状态、键盘和深浅主题。 |
| 声学与时延 | 同一设备与语料对比当前豆包；记录首个有用语音、打断停播、任务结果播报的 p50/p95。 |

无现成时延实测值，不承诺厂商数字。真实语音验收须同时检查声音、工具执行和最终项目状态。自然的口头确认不能替代真实任务结果。[官方语音评估](https://developers.openai.com/cookbook/examples/audio/voice_agent_evaluation)

建议验证命令：新增 crate 后跑 `cargo test -p live_voice`、`cargo test -p agent`、`cargo check -p app`；跨 crate 行为跑 `cargo test --workspace`。GPUI 改动补 release build；页面验收先跑 desktop-e2e，未覆盖部分再用 Computer Use。按实际修改范围执行性能、存储边界和源码行数护栏。

**本次完成的是源码和官方文档调研。** 尚未实现接入、发起付费通话或验收真实声学效果。先验证真实委派与转写能否可靠关联。再验证外放和弱网下能否及时停止持续输出。

完整官方协议核对记录见 [GPT-Live 官方接入调研](../research/2026-10-18-gpt-live-official-integration.md)。
