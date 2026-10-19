---
title: Norma × GPT-Live 实时双工接入方案
subtitle: 参考桌面豆包实现，接入真正的持续听说与后台 Agent 委派
template: doc
theme: paper
lang: zh
date: 2026-10-18
status: 软件功能与自动验证完成 · 真人与云端项目验收延期
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
| 基准版本模式只有 Legacy / Doubao | `crates/app/src/settings/voice/state/mode.rs:2` | 新增 GptLive；不改变旧配置默认值。 |

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

图中Live域层、NormaSession委派、App音频、桌面工具和UI接线均已实施，并有本地整链测试。项目工具继续使用既有执行路径。Live 前端不维护第二套 Canvas、Terminal 或 AgentCard 操作。

```tree
+ crates/live_voice/ | 新增，零 GPUI / Storage 的 Live 能力层
  + src/connection.rs | 异步连接、节奏与有界收发
  + src/session.rs | 官方事件、配置与会话状态机
  + src/context.rs | 双向转写时间线与委派关联
  + src/providers/openai.rs | Live WS 连接与协议
+ crates/app/src/app/gpt_live/ | 采集、播放、后台 Agent 与 UI 编排
+ crates/app/src/app/voice/playback_queue.rs | 从豆包抽取实际共享的播放能力
~ crates/agent/src/master.rs | 接收外部委派上下文和操作关联
~ crates/agent/src/model.rs | 增加明确的 Live 输入来源
~ crates/app/src/settings/voice/ | 模式、凭据、音色与就绪状态
```

新增 crate 只接收音频数据与事件，不访问麦克风、AppDB 或桌面 UI。依赖建议为 `live_voice → audio`，由 `app` 注入配置与音频。不要注册成 ASR/TTS Provider，也不要塞进旧 `DialoguePhase`。

`NormaSession::turn_delegated` 已将模型对话背景与真实用户授权分开，使用独立GptLiveVoice来源；稳定操作ID绑定外部请求、修订与工具序号，相同provider调用ID重放复用原序号。持久化 reservation 仍在 app 后台完成，不能进入 agent 域层。

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
| 持久化去重 | 对委派和工具分别登记；工具键绑定项目、稳定输入意图与操作序号。委派ID和revision用于关联，不另造执行身份。 |
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

协议、连续音频、设置、凭据、委派及持久账本已实施。真实云端双工和App通话已有证据；真实项目工具、物理声学与弱网验收仍未完成。

完整官方协议核对记录见 [GPT-Live 官方接入调研](../research/2026-10-18-gpt-live-official-integration.md)。

## J 当前实施边界

Live没有transcript-done或音频转写完成fence。delegation.offset_ms用于关联，不表示此前输入已转写完毕；append ACK也不能用作写授权。累计上传样本不等于服务端时轴。

生产Bridge从累计已观察原文、近期双向对话和项目状态立即准备语义候选。新用户片段或本地SpeechStart撤销旧规划；SpeechStart之后必须有新增用户文字才可重评。已观察到的撤回和替换优先，引用内容里的更正词不能改变外层动作类别。执行前重验项目、通话代际、revision和取消状态，再通过现有typed-tool授权。

稳定意图使用宿主call身份与首个未消费用户事件身份。未结算副作用阻止并发重做，accepted/unknown保留真实记录。只读查询不进入副作用账本，修订取消后仍保留未执行的写授权。

历史上限512事件、128KiB。滚动窗口保留近期证据，执行中的来源钉住；旧水位事件拒绝。未消费原文被截断时只请求用户重述，不运行项目规划。相同时间戳或钉住证据使窗口无法安全裁剪时，明确Capacity关闭，不能静默丢证据后继续执行。

这些检查约束已观察证据，不能保证未来没有迟到更正。已接受操作进入真实任务生命周期，后续更正通过修正、取消或核对处理。真实项目工具与恢复流程尚未验收，进度见[验收台账](../quality/2026-10-18-gpt-live-acceptance.md)。

## K 实施检查点 · 2026-10-18

| 已落地 | 运行证据与边界 |
| --- | --- |
| 独立GPT-Live设置与凭据 | 真实App模式、缺Key禁用、音色持久化、重启、深浅主题1/1；完整键盘和hover仍待验。 |
| 连续双工媒体循环 | 本地WS解码真PCM，20ms上传与下行同时推进；静音时停止采集并合成零PCM；不代表真实云端或扬声器。 |
| 播放背压 | Live等待队列及硬件未完成PCM合计上限1秒；运输事件队列另有1秒上限。异常回调也等待停止确认。 |
| 配置与生命周期 | 同模式重复选择不断线；跨窗口Key/设备/模式变化停旧通话；Host关窗保留后台也停止语音。 |
| 用量 | 服务端累计快照覆盖显示；无报告只显示未确认；异常worker结束保留最后已知值，不能退回0秒。 |
| 工具桥接 | 已接NormaSession、MemoryAdapter、真实桌面pump和持久账本；observed规划已接线。只读核对入口与持久任务终态已实现，离线整链验证通过；真实云端项目工具仍待统一验收。 |

采用Apple Design的信息层级与即时反馈，复用组件按钮的状态样式，不增加无功能动效。三个模式卡在常用宽度同排，连接操作在首屏可见。真窗口截图与逐图结论见[验收台账](../quality/2026-10-18-gpt-live-acceptance.md)。

官方材料未保证上传累计样本精确等于服务端session时轴。服务端转写和delegation offset用于解释上下文；不能用时钟等式、收包空隙或ACK自动授权项目操作。


## 真实Key验收增量

用户已提供测试Key。真实GPT-Live问候、数字双工、client delegation以及真实App的连接/静音/恢复/挂断已有运行证据。10分钟协议会话未掉线，但服务端最终用量为821秒，本地持续600秒，差异待核对。已移除等待不存在的完整话语信号的设计。真实项目操作、物理声学、完整交互与用量准确性留待用户统一验收。详见[验收台账](../quality/2026-10-18-gpt-live-acceptance.md)。


当前声学检查点：DJI麦克风输入已确认，输出为Mac mini扬声器。已修复工作空间Host遗漏字幕；真实窗口可见一份通话字幕。固定扬声器激励未产生转写，已正常关闭并确认36秒用量；该项记ENV，等待真人语句复验。

耳机路由、真实项目工具与时延样本仍待验。详见[验收台账](../quality/2026-10-18-gpt-live-acceptance.md)。

## L 软件功能补齐

用户要求先完成功能，之后统一验收。软件新增以下能力：

| 能力 | 当前行为 |
| --- | --- |
| 后台反馈 | 配置、超时、预算、模型错误分开说明；草案不冒充执行回执。 |
| 查询结果 | 优先真实状态和稳定ID，再附正文；总注入限制480 UTF-8字节。 |
| 工具准入 | 无项目、缺后台或实际VAD不可用时明确说明，闲聊仍可连接。 |
| 播报控制 | 按钮与语音口令独立停止/恢复声音，麦克风与任务不受影响。 |
| 旧音频清理 | 传输和播放均按代次丢弃停止期间及恢复前的旧声音。 |
| 任务终态 | 稳定task_ref绑定持久任务；完成、失败、取消、中断均取真实DB事实。 |
| 断线补交 | 未确认事实跨断线保留；ACK只表示服务端接收。 |
| 操作核对 | 原命令、不可变回执、任务状态和当前画布按项目只读展示。 |

核对不会自动重做unknown操作。已接受副作用也不会因停止播报而撤回。

离线整链使用本地SSE、NormaSession、MemoryAdapter、真实GPUI pump和画布，验证Browser只创建一次；该证据不代表真实云端或真实窗口。详细结果见验收台账。

最终软件回归合计2142项通过，默认配置GPT聚焦49项通过。最终Release构建与严格Clippy通过；真实设置窗口6步通过，8张图已逐张检查。完整真人双工、云端项目及用量准确性仍待用户统一验收。启动方式见[HTML验收清单](../quality/2026-10-18-gpt-live-user-acceptance.html)。
